#![feature(ip)]

use crate::ipnet::InNet;
use crate::ipnet::ToMac;
use crate::ipnet::TryInNet;
use crate::ipnet::TryToMac;
use std::convert::TryFrom;
use std::convert::TryInto;

use globset::Glob;
use ipnetwork::IpNetwork;
use log::{debug, warn};
use pnet::datalink::{interfaces, MacAddr, NetworkInterface};
use serde_yaml::{Mapping, Value};
use std::io::{self, Write};
use std::net::IpAddr;
use tabwriter::TabWriter;

pub mod ipnet;

pub enum EntryWriteMode {
    DnsMasq,
    Zone,
}

pub struct EntryIterator<'a> {
    mode: EntryWriteMode,
    nets: serde_yaml::mapping::Iter<'a>,
    real_nets: Vec<IpNetwork>,
    real_nets_iter: Option<std::vec::IntoIter<IpNetwork>>,
    hosts_iter: Option<serde_yaml::mapping::Iter<'a>>,
    host: Option<Host>,
}

impl<'a> EntryIterator<'a> {
    pub fn new(data: &'a Mapping, mode: EntryWriteMode) -> Self {
        EntryIterator {
            mode: mode,
            nets: data.iter(),
            real_nets: Vec::new(),
            real_nets_iter: None,
            hosts_iter: None,
            host: None,
        }
    }

    pub fn write<W: io::Write>(&mut self, mut w: W) -> std::io::Result<()> {
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

pub struct Entry {
    name: String,
    mac: Option<MacAddr>,
    ip: IpAddr,
}

impl Entry {
    pub fn as_dnsmasq_host(&self) -> String {
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

    pub fn as_zone_record(&self) -> String {
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
            .filter_map(|o| o.as_mac().cloned()) // mac addresses
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_int().map(|i| i.to_mac())), // integers
            )
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_ip().and_then(|ip| ip.try_to_mac())), // ip addresses
            )
            .nth(0)
    }

    fn get_ip(&self, net: &IpNetwork) -> Option<IpAddr> {
        self.opts
            .iter()
            .filter_map(|o| o.as_ip().filter(|ip| net.contains(**ip)).cloned()) // ips directly in this network
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_ip().filter(|ip| net.is_ipv4() == ip.is_ipv4()))
                    .cloned(), // ips of same family
            )
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_int().map(|i| i.to_mac().in_net(net))), // ints as mac addresses
            )
            .chain(
                self.opts
                    .iter()
                    .filter_map(|o| o.as_mac().map(|mac| mac.in_net(net))), // mac addresses
            )
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

    fn as_int(&self) -> Option<&u64> {
        match self {
            Self::Int(i) => Some(i),
            _ => None,
        }
    }
}

impl TryInNet<IpNetwork, IpAddr> for HostOpt {
    fn try_in_net(self, net: &IpNetwork) -> Option<IpAddr> {
        let ip = match self {
            Self::Ip(v) => v.try_in_net(net),
            Self::Mac(v) => v.try_in_net(net),
            Self::Int(v) => v.try_in_net(net),
        };
        debug!("returning ip: {:?}", ip);
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
    let mut v = interfaces_from_selector(value)
        .into_iter()
        .map(|iface| iface.real_nets())
        .flatten()
        .inspect(|n| debug!("real net collected: {:?}", n))
        .collect::<Vec<_>>();
    v.sort();
    v.dedup();
    v
}

#[derive(Clone)]
struct InterfaceNetwork {
    iface: NetworkInterface,
    network: IpNetwork,
}

impl InterfaceNetwork {
    fn new(iface: NetworkInterface, network: IpNetwork) -> Self {
        Self { iface, network }
    }
    fn all() -> Vec<Self> {
        interfaces()
            .iter()
            .map(|i| {
                i.ips
                    .iter()
                    .map(move |net| Self::new(i.clone(), net.clone()))
            })
            .flatten()
            .collect()
    }

    fn filtered(selector: &Value) -> Vec<Self> {
        Self::filter_networks(&Self::all(), selector)
    }

    fn filter_networks(networks: &Vec<Self>, selector: &Value) -> Vec<Self> {
        if let Some(seq) = selector.as_sequence() {
            return seq
                .iter()
                .map(|v| Self::filter_networks(networks, v))
                .flatten()
                .collect();
        }

        if let Some(map) = selector.as_mapping() {
            return map
                .iter()
                .map(|(selector, filter)| {
                    Self::filter_networks(
                        &Self::filter_networks(networks, selector),
                        filter,
                    )
                })
                .flatten()
                .collect();
        }

        if let Some(i) = selector.as_u64().and_then(|x| u32::try_from(x).ok()) {
            return networks
                .into_iter()
                .filter(|x| x.iface.index == i)
                .cloned()
                .collect();
        }

        if let Some(s) = selector.as_str() {
            match s.to_lowercase().as_ref() {
                "v4" | "ip4" | "ipv4" => {
                    return networks
                        .into_iter()
                        .filter(|x| x.network.is_ipv4())
                        .cloned()
                        .collect()
                }
                "v6" | "ip6" | "ipv6" => {
                    return networks
                        .into_iter()
                        .filter(|x| x.network.is_ipv6())
                        .cloned()
                        .collect()
                }
                _ => {}
            };

            if let Ok(net) = s.parse::<IpNetwork>() {
                return networks
                    .into_iter()
                    .filter(|x| net.contains(x.network.ip()))
                    .cloned()
                    .collect();
            }

            if let Ok(glob) = Glob::new(s) {
                let glob = glob.compile_matcher();
                return networks
                    .into_iter()
                    .filter(|x| glob.is_match(&x.iface.name))
                    .cloned()
                    .collect();
            }

            return networks
                .into_iter()
                .filter(|x| x.iface.name == s)
                .cloned()
                .collect();
        }

        Vec::new()
    }
}

fn interfaces_from_selector(value: &Value) -> Vec<NetworkInterface> {
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
