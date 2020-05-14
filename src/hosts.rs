use crate::ipnet::{TryInNet, TryToMac};
use crate::network::InterfaceNetwork;
use crate::tags::Tags;
use ipnetwork::IpNetwork;
use log::warn;
use pnet::datalink::MacAddr;
use serde_yaml::{Mapping, Value};
use std::convert::TryFrom;
use std::net::IpAddr;

pub struct Host {
    pub name: String,
    opts: Opts,
}

impl Host {
    pub fn new(name: String, opts: Value, tags: Tags) -> Self {
        Self {
            name: name.to_string(),
            opts: Opts::from_vals(opts, &tags),
        }
    }

    pub fn new_hosts(val: Value, tags: Tags) -> impl Iterator<Item = Self> {
        match val {
            Value::Sequence(seq) => Self::new_hosts_from_seq(seq, tags),
            _ => Self::new_hosts_from_seq(vec![val], tags),
        }
    }

    fn new_hosts_from_seq(seq: serde_yaml::Sequence, tags: Tags) -> impl Iterator<Item = Self> {
        seq.into_iter()
            .filter_map(move |v| match v {
                Value::Mapping(map) => Some(Self::new_hosts_from_map(map, tags.clone())),
                _ => {
                    warn!("invalid host map: {:?}", v);
                    None
                }
            })
            .flatten()
    }

    fn new_hosts_from_map(map: Mapping, tags: Tags) -> impl Iterator<Item = Self> {
        let mut tags = tags;
        map.into_iter().filter_map(move |(k, v)| match k {
            Value::String(name) => {
                if name.starts_with("_tag") {
                    tags = tags.new_child(&v);
                }
                Some(Self::new(name, v, tags.clone()))
            }
            _ => {
                warn!("invalid host name: {:?}", k);
                None
            }
        })
    }

    pub fn get_mac(&self, net: &InterfaceNetwork, tags: &Tags) -> Option<MacAddr> {
        self.get_mac_with_tags(net, tags).map(|(mac, _)| mac)
    }

    pub fn get_mac_with_tags<'a>(
        &'a self,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(MacAddr, &'a Tags)> {
        self.opts.get_mac_with_tags(net, tags)
    }

    pub fn get_ip(&self, net: &InterfaceNetwork, tags: &Tags) -> Option<IpAddr> {
        self.get_ip_with_tags(net, tags).map(|(ip, _)| ip)
    }

    pub fn get_ip_with_tags<'a>(
        &'a self,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(IpAddr, &'a Tags)> {
        self.opts.get_ip_with_tags(net, tags)
    }
}

pub struct Opts {
    opts: Vec<OptVal>,
    tags: Tags,
}

impl Opts {
    fn new(opts: Vec<OptVal>, tags: Tags) -> Self {
        Self { opts, tags }
    }

    fn from_vals(val: Value, tags: &Tags) -> Self {
        let tags = tags.extract(&val);
        Self::new(OptVal::from_vals(val, &tags), tags)
    }

    fn get_mac_with_tags<'a>(
        &'a self,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(MacAddr, &'a Tags)> {
        if self.tags.matches(tags) {
            OptVal::get_mac_with_tags(&self.opts, net, tags)
        } else {
            None
        }
    }

    fn get_mac(&self, net: &InterfaceNetwork, tags: &Tags) -> Option<MacAddr> {
        self.get_mac_with_tags(net, tags).map(|(mac, _)| mac)
    }

    fn get_ip_with_tags<'a>(
        &'a self,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(IpAddr, &'a Tags)> {
        if self.tags.matches(tags) {
            OptVal::get_ip_with_tags(&self.opts, net, tags)
        } else {
            None
        }
    }
}

pub enum OptVal {
    Labeled(Label),
    Mac(MacAddr),
    IpNet(IpNetwork),
    Int(u64),
    Iface,
}

pub enum Label {
    Mac(Opts),
    Ipv4(Opts),
    Ipv6(Opts),
    Ip(Opts),
}

impl TryFrom<(Value, Value, &Tags)> for Label {
    type Error = ();
    fn try_from((k, v, t): (Value, Value, &Tags)) -> Result<Self, Self::Error> {
        if let Some(s) = k.as_str() {
            match s.to_lowercase().as_ref() {
                "mac" => Ok(Self::Mac(Opts::from_vals(v, t))),
                "ip4" | "ipv4" => Ok(Self::Ipv4(Opts::from_vals(v, t))),
                "ip6" | "ipv6" => Ok(Self::Ipv6(Opts::from_vals(v, t))),
                "ip" => Ok(Self::Ip(Opts::from_vals(v, t))),
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

impl OptVal {
    fn from_vals(val: Value, tags: &Tags) -> Vec<OptVal> {
        match val {
            Value::Sequence(s) => {
                return s
                    .into_iter()
                    .map(|v| Self::from_vals(v, tags))
                    .flatten()
                    .collect()
            }
            Value::Mapping(m) => {
                return m
                    .into_iter()
                    .filter_map(|(k, v)| {
                        Label::try_from((k, v, tags)).map(|l| Self::Labeled(l)).ok()
                    })
                    .collect()
            }
            _ => {}
        };

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

    fn get_mac_with_tags<'a>(
        opts: &'a Vec<OptVal>,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(MacAddr, &'a Tags)> {
        // try labeled options
        if let Some(o) = opts
            .iter()
            .filter_map(|o| match o {
                Self::Labeled(Label::Mac(mac_opts)) => Some(mac_opts),
                _ => None,
            })
            .nth(0)
        {
            return o.get_mac_with_tags(net, tags);
        }

        return Self::get_mac(opts, net).map(|mac| (mac, tags));
    }

    fn get_mac(opts: &Vec<OptVal>, net: &InterfaceNetwork) -> Option<MacAddr> {
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
                    Self::Iface => net.iface.as_ref().and_then(|i| i.mac),
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

    fn get_ip_with_tags<'a>(
        opts: &'a Vec<OptVal>,
        net: &InterfaceNetwork,
        tags: &'a Tags,
    ) -> Option<(IpAddr, &'a Tags)> {
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
                return o.get_ip_with_tags(net, tags);
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
                return o.get_ip_with_tags(net, tags);
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
            return o.get_ip_with_tags(net, tags);
        }

        Self::get_ip(opts, net).map(|ip| (ip, tags))
    }

    fn get_ip(opts: &Vec<OptVal>, net: &InterfaceNetwork) -> Option<IpAddr> {
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
