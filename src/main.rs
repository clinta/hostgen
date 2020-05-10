#![feature(ip)]

use std::convert::TryFrom;
use std::convert::TryInto;

use clap::{App, Arg, SubCommand};
use globset::Glob;
use ipnetwork::IpNetwork;
use log::{debug, warn};
use pnet::datalink::{interfaces, MacAddr, NetworkInterface};
use serde_yaml::{Mapping, Value};
use std::fs::File;
use std::io::{self, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use tabwriter::TabWriter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    let matches = App::new("Host Config Generator")
        .version("0.1")
        .author("Clint Armstrong <clint@clintarmstrong.net>")
        .about("Generates dnsmasq and zonec configs")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("config file")
                .takes_value(true)
                .index(1),
        )
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILE")
                .help("output file")
                .takes_value(true),
        )
        .subcommand(SubCommand::with_name("dnsmasq").about("generates dnsmasq hosts"))
        .subcommand(SubCommand::with_name("zone").about("generates zone entries"))
        .get_matches();

    let f = std::fs::File::open(matches.value_of("config").unwrap_or("hosts.yaml"))?;
    let data: Mapping = serde_yaml::from_reader(f)?;

    /*
    let mut writer = if let Some(output) = matches.value_of("output") {
        File::create(output)?
    } else {
        return Err(());
        /*
    let stdout = io::stdout();
    stdout.lock()
    */
    }
    */
    //EntryIterator::new(&data, EntryWriteMode::DnsMasq)
    let mut entries = match matches.subcommand_name() {
        Some("dnsmasq") => EntryIterator::new(&data, EntryWriteMode::DnsMasq),
        Some("zone") => EntryIterator::new(&data, EntryWriteMode::Zone),
        _ => return Ok(()),
    };

    if let Some(output) = matches.value_of("output") {
        let mut writer = File::create(output)?;
        entries.write(&mut writer)?;
    } else {
        let stdout = io::stdout();
        let mut writer = stdout.lock();
        entries.write(&mut writer)?;
    }

    Ok(())
}

enum EntryWriteMode {
    DnsMasq,
    Zone,
}

struct EntryIterator<'a> {
    mode: EntryWriteMode,
    nets: serde_yaml::mapping::Iter<'a>,
    real_nets: Vec<IpNetwork>,
    real_nets_iter: Option<std::vec::IntoIter<IpNetwork>>,
    hosts_iter: Option<serde_yaml::mapping::Iter<'a>>,
    host: Option<Host>,
}

impl<'a> EntryIterator<'a> {
    fn new(data: &'a Mapping, mode: EntryWriteMode) -> Self {
        EntryIterator {
            mode: mode,
            nets: data.iter(),
            real_nets: Vec::new(),
            real_nets_iter: None,
            hosts_iter: None,
            host: None,
        }
    }

    fn write<W: io::Write>(&mut self, mut w: W) -> std::io::Result<()> {
        match self.mode {
            EntryWriteMode::DnsMasq => self.write_dnsmasq_hosts(&mut w),
            EntryWriteMode::Zone => self.write_zone_records(&mut w),
        }
    }

    fn write_dnsmasq_hosts<W: io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
        for entry in self {
            writeln!(w, "{}", entry.as_dnsmasq_host())?;
        }
        Ok(())
    }

    fn write_zone_records<W: io::Write>(&mut self, w: &mut W) -> std::io::Result<()> {
        let mut w = TabWriter::new(w);
        for entry in self {
            writeln!(w, "{}", entry.as_zone_record())?;
        }
        w.flush()?;
        Ok(())
    }

    fn next_real_net(&mut self) -> Option<IpNetwork> {
        self.real_nets_iter.as_mut().and_then(|x| x.next())
    }

    fn next_host_val(&mut self) -> Option<(&Value, &Value)> {
        self.hosts_iter.as_mut().and_then(|x| x.next())
    }

    fn next_host(&mut self) -> Option<Host> {
        if let Some((host, host_opts)) = self.next_host_val() {
            if let (Some(host), Some(host_opts)) = (host.as_str(), host_opts.as_sequence()) {
                Some(Host {
                    name: host.to_string(),
                    opts: host_opts.iter().filter_map(|x| x.try_into().ok()).collect(),
                })
            } else {
                warn!("invalid host name or opts: {:?}: {:?}", host, host_opts);
                self.next_host()
            }
        } else {
            let (net, hosts) = self.nets.next()?; // return none if out of nets
            self.hosts_iter = hosts.as_mapping().map(|x| x.iter());
            self.real_nets = net_to_real_nets(&net);
            self.next_host()
        }
    }
}

impl<'a> Iterator for EntryIterator<'a> {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let (Some(net), Some(host)) = (self.next_real_net(), self.host.as_ref()) {
            host.as_entry(&net)
        } else {
            self.host = Some(self.next_host()?);
            self.real_nets_iter = Some(self.real_nets.clone().into_iter());
            self.next()
        }
    }
}

struct Entry {
    name: String,
    mac: Option<MacAddr>,
    ip: IpAddr,
}

impl Entry {
    fn as_dnsmasq_host(&self) -> String {
        let mut elems = Vec::new();
        if let Some(mac) = self.mac {
            elems.push(mac.to_string());
        }
        if self.ip.is_ipv6() {
            elems.push("[".to_string() + &self.ip.to_string() + "]");
        } else {
            elems.push(self.ip.to_string());
        }
        elems.push(self.name.to_string());
        elems.join(",")
    }

    fn as_zone_record(&self) -> String {
        let mut elems = Vec::new();
        elems.push(self.name.to_string());
        if self.ip.is_ipv6() {
            elems.push("AAAA".to_string());
        } else {
            elems.push("A".to_string());
        }
        elems.push(self.ip.to_string());
        elems.join("\t")
    }
}

struct Host {
    name: String,
    opts: Vec<HostOpt>,
}

impl Host {
    fn get_mac(&self) -> Option<MacAddr> {
        self.opts
            .iter()
            .filter_map(|o| o.as_mac().cloned())
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_int().map(|i| i.to_mac())),
            )
            .nth(0)
    }

    fn get_ip(&self, net: &IpNetwork) -> Option<IpAddr> {
        self.opts
            .iter()
            .filter(|o| o.as_ip().map(|ip| net.contains(*ip)).unwrap_or(false)) // ips directly in this network
            .chain(
                self.opts.iter().filter(|o| {
                    o.as_ip()
                        .map(|ip| net.is_ipv4() == ip.is_ipv4())
                        .unwrap_or(false)
                }), // ips of same family
            )
            .chain(
                self.opts.iter().filter(|o| o.is_int()), // ints
            )
            .chain(
                self.opts.iter().filter(|o| o.is_mac()), // mac addresses
            )
            .map(|o| o.to_ip(net))
            .nth(0)
    }

    fn as_entry(&self, net: &IpNetwork) -> Option<Entry> {
        let ip = self.get_ip(net)?;
        Some(Entry {
            name: self.name.clone(),
            mac: self.get_mac(),
            ip: ip,
        })
    }
}

enum HostOpt {
    Int(u64),
    Mac(MacAddr),
    Ip(IpAddr),
}

impl HostOpt {
    fn is_mac(&self) -> bool {
        self.as_mac().is_some()
    }

    fn as_mac(&self) -> Option<&MacAddr> {
        match self {
            Self::Mac(mac) => Some(mac),
            _ => None,
        }
    }

    fn as_ip(&self) -> Option<&IpAddr> {
        match self {
            Self::Ip(ip) => Some(ip),
            _ => None,
        }
    }

    fn is_int(&self) -> bool {
        self.as_int().is_some()
    }

    fn as_int(&self) -> Option<&u64> {
        match self {
            Self::Int(i) => Some(i),
            _ => None,
        }
    }
}

impl ToIp for HostOpt {
    fn to_ip(&self, net: &IpNetwork) -> IpAddr {
        let ip = match self {
            Self::Ip(v) => v.to_ip(net),
            Self::Mac(v) => v.to_ip(net),
            Self::Int(v) => v.to_ip(net),
        };
        debug!("returning ip: {}", ip);
        ip
    }
}

impl TryFrom<&str> for HostOpt {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if let Ok(m) = s.parse::<MacAddr>() {
            return Ok(Self::Mac(m));
        }

        if let Ok(ip) = s.parse::<IpAddr>() {
            return Ok(Self::Ip(ip));
        }

        if let Ok(n) = s.parse::<u64>() {
            return Ok(Self::Int(n));
        }

        warn!("unable to convert value to host opt: {:?}", s);
        Err(())
    }
}

impl TryFrom<&Value> for HostOpt {
    type Error = ();
    fn try_from(v: &Value) -> Result<Self, Self::Error> {
        if let Some(n) = v.as_u64() {
            return Ok(Self::Int(n));
        }

        if let Some(s) = v.as_str() {
            return s.try_into();
        }

        warn!("unable to convert value to host opt: {:?}", v);
        Err(())
    }
}

fn net_to_real_nets(value: &Value) -> Vec<IpNetwork> {
    let mut v = net_to_ifaces(value)
        .into_iter()
        .map(|iface| iface.real_nets())
        .flatten()
        .inspect(|n| debug!("real net collected: {:?}", n))
        .collect::<Vec<_>>();
    v.sort();
    v.dedup();
    v
}

fn net_to_ifaces(value: &Value) -> Vec<NetworkInterface> {
    if let Some(i) = value.as_u64().and_then(|x| u32::try_from(x).ok()) {
        return interfaces().into_iter().filter(|x| x.index == i).collect();
    }

    if let Some(s) = value.as_str() {
        if let Ok(net) = s.parse::<IpNetwork>() {
            return interfaces()
                .into_iter()
                .filter(|x| x.ips.iter().any(|ip| net.contains(ip.ip())))
                .collect();
        }

        if let Ok(glob) = Glob::new(s) {
            let glob = glob.compile_matcher();
            return interfaces()
                .into_iter()
                .filter(|x| glob.is_match(&x.name))
                .collect();
        }

        return interfaces().into_iter().filter(|x| x.name == s).collect();
    }

    Vec::new()
}

trait RealNets {
    fn real_nets(&self) -> Vec<IpNetwork>;
}

impl RealNets for NetworkInterface {
    fn real_nets(&self) -> Vec<IpNetwork> {
        self.ips
            .iter()
            .filter(|ip| {
                ip.ip().is_global()
                    || match ip.ip() {
                        IpAddr::V4(v4) => v4.is_private(),
                        IpAddr::V6(v6) => v6.is_unique_local(),
                    }
            })
            .filter_map(|ip| IpNetwork::with_netmask(ip.network(), ip.mask()).ok())
            .collect()
    }
}

trait ToMac {
    fn to_mac(&self) -> MacAddr;
}

impl ToMac for u64 {
    fn to_mac(&self) -> MacAddr {
        debug!("converting u64 to mac: {}", self);
        let [_, _, a, b, c, d, e, f] = self.to_be_bytes();
        let a = a | 0b0000_0010; // set local managed bit
        MacAddr::new(a, b, c, d, e, f)
    }
}

trait ToIp {
    fn to_ip(&self, net: &IpNetwork) -> IpAddr;
}

impl ToIp for MacAddr {
    fn to_ip(&self, net: &IpNetwork) -> IpAddr {
        debug!("converting mac to ip: {} in {}", self, net);
        match net {
            IpNetwork::V4(v4) => {
                let (ip, mask) = (v4.ip().octets(), v4.mask().octets());
                IpAddr::V4(Ipv4Addr::new(
                    (ip[0] & mask[0]) | (self.2 & !mask[0]),
                    (ip[1] & mask[1]) | (self.3 & !mask[1]),
                    (ip[2] & mask[2]) | (self.4 & !mask[2]),
                    (ip[3] & mask[3]) | (self.5 & !mask[3]),
                ))
            }
            IpNetwork::V6(v6) => {
                let (ip, mask) = (v6.ip().octets(), v6.mask().octets());
                IpAddr::V6(Ipv6Addr::from([
                    ip[0] & mask[0],
                    ip[1] & mask[1],
                    ip[2] & mask[2],
                    ip[3] & mask[3],
                    ip[4] & mask[4],
                    ip[5] & mask[5],
                    ip[6] & mask[6],
                    ip[7] & mask[7],
                    (ip[8] & mask[8]) | ((self.0 ^ 0b0000_0010) & !mask[8]), // flip local managed bit
                    (ip[9] & mask[9]) | (self.1 & !mask[9]),
                    (ip[10] & mask[10]) | (self.2 & !mask[10]),
                    (ip[11] & mask[11]) | (0xff & !mask[11]),
                    (ip[12] & mask[12]) | (0xfe & !mask[12]),
                    (ip[13] & mask[13]) | (self.3 & !mask[13]),
                    (ip[14] & mask[14]) | (self.4 & !mask[14]),
                    (ip[15] & mask[15]) | (self.5 & !mask[15]),
                ]))
            }
        }
    }
}

impl ToIp for u64 {
    fn to_ip(&self, net: &IpNetwork) -> IpAddr {
        debug!("converting u64 to ip: {:x?} in {}", self, net);
        self.to_mac().to_ip(net)
    }
}

impl ToIp for IpAddr {
    fn to_ip(&self, net: &IpNetwork) -> IpAddr {
        debug!("converting ip to ip: {} in {}", self, net);
        let v6 = match self {
            IpAddr::V6(v6) => *v6,
            IpAddr::V4(v4) => v4.to_ipv6_compatible(),
        };
        let b = v6.octets();
        match net {
            IpNetwork::V4(v4) => {
                let (ip, mask) = (v4.ip().octets(), v4.mask().octets());
                IpAddr::V4(Ipv4Addr::new(
                    (ip[0] & mask[0]) | (b[12] & !mask[0]),
                    (ip[1] & mask[1]) | (b[13] & !mask[1]),
                    (ip[2] & mask[2]) | (b[14] & !mask[2]),
                    (ip[3] & mask[3]) | (b[15] & !mask[3]),
                ))
            }
            IpNetwork::V6(v6) => {
                let (ip, mask) = (v6.ip().octets(), v6.mask().octets());
                IpAddr::V6(Ipv6Addr::from([
                    (ip[0] & mask[0]) | (b[0] & !mask[0]),
                    (ip[1] & mask[1]) | (b[1] & !mask[1]),
                    (ip[2] & mask[2]) | (b[2] & !mask[2]),
                    (ip[3] & mask[3]) | (b[3] & !mask[3]),
                    (ip[4] & mask[4]) | (b[4] & !mask[4]),
                    (ip[5] & mask[5]) | (b[5] & !mask[5]),
                    (ip[6] & mask[6]) | (b[6] & !mask[6]),
                    (ip[7] & mask[7]) | (b[7] & !mask[7]),
                    (ip[8] & mask[8]) | (b[8] & !mask[8]),
                    (ip[9] & mask[9]) | (b[9] & !mask[9]),
                    (ip[10] & mask[10]) | (b[10] & !mask[10]),
                    (ip[11] & mask[11]) | (b[11] & !mask[11]),
                    (ip[12] & mask[12]) | (b[12] & !mask[12]),
                    (ip[13] & mask[13]) | (b[13] & !mask[13]),
                    (ip[14] & mask[14]) | (b[14] & !mask[14]),
                    (ip[15] & mask[15]) | (b[15] & !mask[15]),
                ]))
            }
        }
    }
}
