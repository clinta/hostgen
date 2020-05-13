use globset::Glob;
use hostgen::ipnet::InNet;
use hostgen::ipnet::ToMac;
use hostgen::ipnet::TryInNet;
use hostgen::ipnet::TryToMac;
use hostgen::InterfaceNetwork;
use ipnetwork::IpNetwork;
use log::warn;
use pnet::datalink::{interfaces, MacAddr, NetworkInterface};
use serde_yaml::{Mapping, Value};
use std::convert::TryInto;
use std::convert::{From, TryFrom};
use std::io::{self, Write};
use std::iter;
use std::iter::FromIterator;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;
use tabwriter::TabWriter;

pub struct Host {
    pub name: String,
    opts: Vec<Opt>,
}

impl Host {
    pub fn new(name: &str, opts: &Value) -> Self {
        Self {
            name: name.to_string(),
            opts: Opt::opts_from_vals(opts),
        }
    }

    pub fn get_mac(&self, net: &InterfaceNetwork) -> Option<MacAddr> {
        Opt::get_mac(&self.opts, net)
    }

    pub fn ip(&self, net: &InterfaceNetwork) -> Option<IpAddr> {
        Opt::get_ip(&self.opts, net)
    }
}

pub enum Opt {
    Labeled(Label),
    Mac(MacAddr),
    IpNet(IpNetwork),
    Int(u64),
    Iface,
}

pub enum Label {
    Mac(Vec<Opt>),
    Ipv4(Vec<Opt>),
    Ipv6(Vec<Opt>),
    Ip(Vec<Opt>),
}

impl TryFrom<(&Value, &Value)> for Label {
    type Error = ();
    fn try_from((k, v): (&Value, &Value)) -> Result<Self, Self::Error> {
        if let Some(s) = k.as_str() {
            match s.to_lowercase().as_ref() {
                "mac" => Ok(Self::Mac(Opt::opts_from_vals(v))),
                "ip4" | "ipv4" => Ok(Self::Ipv4(Opt::opts_from_vals(v))),
                "ip6" | "ipv6" => Ok(Self::Ipv6(Opt::opts_from_vals(v))),
                "ip" => Ok(Self::Ip(Opt::opts_from_vals(v))),
                _ => {
                    warn!("unknown label key: {}", s);
                    Err(())
                }
            }
        } else {
            warn!("unknown label key: {:?}", k);
            Err(())
        }
    }
}

impl Opt {
    fn opts_from_vals(val: &Value) -> Vec<Opt> {
        if let Some(s) = val.as_sequence() {
            return s
                .iter()
                .map(|v| Self::opts_from_vals(v))
                .flatten()
                .collect();
        }
        if let Some(m) = val.as_mapping() {
            return m
                .iter()
                .filter_map(|i| Label::try_from(i).map(|l| Self::Labeled(l)).ok())
                .collect();
        }
        if let Some(i) = val.as_u64() {
            return vec![Self::Int(i)];
        }
        if let Some(s) = val.as_str() {
            if s.to_lowercase() == "iface" {
                return vec![Self::Iface];
            }
            if let Ok(m) = s.parse::<MacAddr>() {
                return vec![Self::Mac(m)];
            }
            if let Ok(ip) = s.parse::<IpNetwork>() {
                return vec![Self::IpNet(ip)];
            }
            if let Ok(i) = s.parse::<u64>() {
                return vec![Self::Int(i)];
            }
        }
        warn!("unable to convert val: {:?}", val);
        vec![]
    }

    fn get_mac(opts: &Vec<Opt>, net: &InterfaceNetwork) -> Option<MacAddr> {
        // try labeled options
        if let Some(o) = opts
            .iter()
            .filter_map(|o| match o {
                Self::Labeled(Label::Mac(mac_opts)) => Some(mac_opts),
                _ => None,
            })
            .nth(0)
        {
            return Self::get_mac(o, net);
        }

        opts.iter()
            .filter_map(|o| {
                // parsed macs
                match o {
                    Self::Mac(mac) => Some(mac.clone()),
                    _ => None,
                }
            })
            .chain(opts.iter().filter_map(|o| {
                // interfaces
                match o {
                    Self::Iface => net.iface.mac,
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // integers
                match o {
                    Self::Int(i) => i.try_to_mac(),
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // ipv6 addresses
                match o {
                    Self::IpNet(IpNetwork::V6(v6)) => v6.ip().try_to_mac(),
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // ipv4 addresses
                match o {
                    Self::IpNet(IpNetwork::V4(v4)) => v4.ip().try_to_mac(),
                    _ => None,
                }
            }))
            .nth(0)
    }

    fn get_ip(opts: &Vec<Opt>, net: &InterfaceNetwork) -> Option<IpAddr> {
        if net.network.is_ipv4() {
            // try labeled ipv4 options
            if let Some(o) = opts
                .iter()
                .filter_map(|o| match o {
                    Self::Labeled(Label::Ipv4(ip_opts)) => Some(ip_opts),
                    _ => None,
                })
                .nth(0)
            {
                return Self::get_ip(o, net);
            }
        }

        if net.network.is_ipv6() {
            // try labeled ipv6 options
            if let Some(o) = opts
                .iter()
                .filter_map(|o| match o {
                    Self::Labeled(Label::Ipv6(ip_opts)) => Some(ip_opts),
                    _ => None,
                })
                .nth(0)
            {
                return Self::get_ip(o, net);
            }
        }

        // try labeled ip options
        if let Some(o) = opts
            .iter()
            .filter_map(|o| match o {
                Self::Labeled(Label::Ip(ip_opts)) => Some(ip_opts),
                _ => None,
            })
            .nth(0)
        {
            return Self::get_ip(o, net);
        }

        opts.iter()
            .filter_map(|o| {
                // parsed ips in same network
                match o {
                    Self::IpNet(ip) => {
                        if net.network.contains(ip.ip()) {
                            Some(ip.ip())
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            })
            .chain(opts.iter().filter_map(|o| {
                // parsed ips in same family
                match o {
                    Self::IpNet(ip) => {
                        if net.network.is_ipv4() == ip.is_ipv4() {
                            ip.ip().try_in_net(&net.network)
                        } else {
                            None
                        }
                    }
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // interfaces
                match o {
                    Self::Iface => net.network.ip().try_in_net(&net.network),
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // integers
                match o {
                    Self::Int(i) => i.try_in_net(&net.network),
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // mac addresses
                match o {
                    Self::Mac(mac) => mac.try_in_net(&net.network),
                    _ => None,
                }
            }))
            .chain(opts.iter().filter_map(|o| {
                // any ip addresses
                match o {
                    Self::IpNet(ip) => ip.ip().try_in_net(&net.network),
                    _ => None,
                }
            }))
            .nth(0)
    }
}
