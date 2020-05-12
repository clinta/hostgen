#![feature(ip)]

use crate::ipnet::InNet;
use crate::ipnet::ToMac;
use crate::ipnet::TryInNet;
use crate::ipnet::TryToMac;
use std::convert::TryInto;
use std::convert::{From, TryFrom};

use globset::Glob;
use ipnetwork::IpNetwork;
use log::warn;
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
    data: serde_yaml::mapping::Iter<'a>,
    networks: Vec<InterfaceNetwork>,
    networks_iter: Option<std::vec::IntoIter<InterfaceNetwork>>,
    hosts_iter: Option<serde_yaml::mapping::Iter<'a>>,
    host: Option<Host>,
}

impl<'a> EntryIterator<'a> {
    pub fn new(data: &'a Mapping, mode: EntryWriteMode) -> Self {
        EntryIterator {
            mode: mode,
            data: data.iter(),
            networks: Vec::new(),
            networks_iter: None,
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

    fn next_network(&mut self) -> Option<InterfaceNetwork> {
        self.networks_iter.as_mut().and_then(|x| x.next())
    }

    fn next_host_val(&mut self) -> Option<(&Value, &Value)> {
        self.hosts_iter.as_mut().and_then(|x| x.next())
    }

    fn next_host(&mut self) -> Option<Host> {
        if let Some((host, host_opts)) = self.next_host_val() {
            if let Some(host) = host.as_str() {
                Some(Host::new(host, host_opts))
            } else {
                warn!("invalid host name or opts: {:?}: {:?}", host, host_opts);
                self.next_host()
            }
        } else {
            let (filter, hosts) = self.data.next()?; // return none if out of data
            self.hosts_iter = hosts.as_mapping().map(|x| x.iter());
            self.networks = InterfaceNetwork::filtered(&filter);
            self.next_host()
        }
    }
}

impl<'a> Iterator for EntryIterator<'a> {
    type Item = Entry;

    fn next(&mut self) -> Option<Self::Item> {
        if let (Some(net), Some(host)) = (self.next_network(), self.host.as_ref()) {
            if let Some(entry) = host.as_entry(&net) {
                Some(entry)
            } else {
                self.next() // go to next network is None due to bad network -> ip
            }
        } else {
            self.host = Some(self.next_host()?);
            self.networks_iter = Some(self.networks.clone().into_iter());
            self.next()
        }
    }
}

#[derive(Debug, Clone)]
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

#[derive(Debug)]
struct Host {
    name: String,
    opts: Vec<HostOpt>,
}

impl Host {
    fn new(name: &str, opts: &Value) -> Self {
        Self {
            name: name.to_string(),
            opts: HostOpt::new_opts(opts),
        }
    }

    fn get_mac(&self, net: &InterfaceNetwork) -> Option<MacAddr> {
        HostOpt::get_opts_mac(&self.opts, net)
    }

    fn get_ip(&self, net: &InterfaceNetwork) -> Option<IpAddr> {
        HostOpt::get_opts_ip(&self.opts, net)
    }

    fn as_entry(&self, net: &InterfaceNetwork) -> Option<Entry> {
        let ip = self.get_ip(net)?;
        Some(Entry {
            name: self.name.clone(),
            mac: self.get_mac(net),
            ip: ip,
        })
    }
}

#[derive(Debug)]
enum HostOpt {
    Int(u64),
    Mac(MacAddr),
    Ip(IpAddr),
    Specific(SpecificOpt),
}

#[derive(Debug)]
enum SpecificOpt {
    Mac(Vec<HostOpt>),
    Ipv4(Vec<HostOpt>),
    Ipv6(Vec<HostOpt>),
    Ip(Vec<HostOpt>),
    Iface(Value),
}

impl SpecificOpt {
    fn try_new(k: &Value, v: &Value) -> Option<Self> {
        match k.as_str().unwrap_or("").to_lowercase().as_ref() {
            "mac" => Some(Self::Mac(HostOpt::new_opts(v))),
            "v4" | "ip4" | "ipv4" => Some(Self::Ipv4(HostOpt::new_opts(v))),
            "v6" | "ip6" | "ipv6" => Some(Self::Ipv6(HostOpt::new_opts(v))),
            "ip" => Some(Self::Ip(HostOpt::new_opts(v))),
            "iface" => Some(Self::Iface(v.clone())),
            _ => None,
        }
    }

    fn as_mac(&self) -> Option<&Vec<HostOpt>> {
        match self {
            Self::Mac(v) => Some(v),
            _ => None,
        }
    }

    fn as_ipv4(&self) -> Option<&Vec<HostOpt>> {
        match self {
            Self::Ipv4(v) => Some(v),
            _ => None,
        }
    }

    fn as_ipv6(&self) -> Option<&Vec<HostOpt>> {
        match self {
            Self::Ipv6(v) => Some(v),
            _ => None,
        }
    }

    fn as_ip(&self) -> Option<&Vec<HostOpt>> {
        match self {
            Self::Ip(v) => Some(v),
            _ => None,
        }
    }

    fn as_iface(&self) -> Option<&Value> {
        match self {
            Self::Iface(v) => Some(v),
            _ => None,
        }
    }
}

impl HostOpt {
    fn new_opts(v: &Value) -> Vec<HostOpt> {
        if let Some(seq) = v.as_sequence() {
            seq.iter().map(|x| Self::new_opts(x)).flatten().collect()
        } else if let Some(map) = v.as_mapping() {
            map.iter()
                .filter_map(|(k, v)| SpecificOpt::try_new(k, v).map(|x| Self::Specific(x)))
                .collect()
        } else {
            let mut r = Vec::new();
            if let Ok(opt) = v.try_into() {
                r.push(opt);
            }
            r
        }
    }

    fn as_specific(&self) -> Option<&SpecificOpt> {
        match self {
            Self::Specific(spec) => Some(spec),
            _ => None,
        }
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

    fn as_int(&self) -> Option<&u64> {
        match self {
            Self::Int(i) => Some(i),
            _ => None,
        }
    }

    fn as_iface(&self, net: &InterfaceNetwork) -> Option<InterfaceNetwork> {
        self.as_specific()
            .and_then(|s| s.as_iface())
            .and_then(|v| match v {
                Value::Null => Some(net.clone()),
                _ => InterfaceNetwork::filtered(v).first().cloned(),
            })
    }

    fn get_opts_mac(opts: &Vec<HostOpt>, net: &InterfaceNetwork) -> Option<MacAddr> {
        opts.iter()
            .filter_map(|o| o.as_mac().cloned()) // mac addresses
            .chain(
                opts.iter()
                    .filter_map(|o| o.as_iface(net).and_then(|iface| iface.iface.mac)), // mac address from iface
            )
            .chain(
                opts.iter().filter_map(|o| o.as_int().map(|i| i.to_mac())), // integers
            )
            .chain(
                opts.iter()
                    .filter_map(|o| o.as_ip().and_then(|ip| ip.try_to_mac())), // ip addresses
            )
            .nth(0)
    }

    fn get_opts_ip(opts: &Vec<HostOpt>, net: &InterfaceNetwork) -> Option<IpAddr> {
        opts.iter()
            .filter_map(|o| o.as_ip().filter(|ip| net.network.contains(**ip)).cloned()) // ips directly in this network
            .chain(
                opts.iter().filter_map(|o| {
                    o.as_ip()
                        .filter(|ip| net.network.is_ipv4() == ip.is_ipv4())
                        .and_then(|ip| ip.try_in_net(&net.network))
                }), // ips of same family
            )
            .chain(
                opts.iter().filter_map(|o| {
                    o.as_iface(net)
                        .map(|iface| iface.network.ip())
                        .filter(|ip| net.network.contains(*ip))
                }), // iface ips directly in this network
            )
            .chain(
                opts.iter().filter_map(|o| {
                    o.as_iface(net)
                        .map(|iface| iface.network.ip())
                        .filter(|ip| ip.is_ipv4() == net.network.is_ipv4())
                        .and_then(|ip| ip.try_in_net(&net.network))
                }), // iface ips of same family
            )
            .chain(
                opts.iter().filter_map(|o| {
                    o.as_iface(net)
                        .map(|iface| iface.network.ip())
                        .and_then(|ip| ip.try_in_net(&net.network))
                }), // iface ips
            )
            .chain(
                opts.iter()
                    .filter_map(|o| o.as_int().map(|i| i.to_mac().in_net(&net.network))), // ints as mac addresses
            )
            .chain(
                opts.iter()
                    .filter_map(|o| o.as_mac().map(|mac| mac.in_net(&net.network))), // mac addresses
            )
            .nth(0)
    }

    fn ip_in_net(&self, net: &InterfaceNetwork) -> Option<IpAddr> {
        match self {
            Self::Ip(v) => v.try_in_net(&net.network),
            Self::Mac(v) => v.try_in_net(&net.network),
            Self::Int(v) => v.try_in_net(&net.network),
            Self::Specific(s) => match s {
                SpecificOpt::Mac(v) => {
                    HostOpt::get_opts_mac(v, net).and_then(|m| m.try_in_net(&net.network))
                }
                SpecificOpt::Ip(v) => HostOpt::get_opts_ip(v, net),
                SpecificOpt::Ipv4(v) => {
                    if net.network.is_ipv4() {
                        HostOpt::get_opts_ip(v, net)
                    } else {
                        None
                    }
                }
                SpecificOpt::Ipv6(v) => {
                    if net.network.is_ipv6() {
                        HostOpt::get_opts_ip(v, net)
                    } else {
                        None
                    }
                }
                SpecificOpt::Iface(v) => InterfaceNetwork::filtered(v)
                    .first()
                    .and_then(|n| n.network.ip().try_in_net(&net.network)),
            },
        }
    }
}

impl TryFrom<&str> for HostOpt {
    type Error = ();
    fn try_from(s: &str) -> Result<Self, Self::Error> {
        if s.to_lowercase() == "iface" {
            return Ok(Self::Specific(SpecificOpt::Iface(Value::Null)));
        }

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

#[derive(Debug, Clone)]
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
                    Self::filter_networks(&Self::filter_networks(networks, selector), filter)
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
