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

#[derive(Debug, Clone)]
pub struct InterfaceNetwork {
    pub iface: NetworkInterface,
    pub network: IpNetwork,
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

    pub fn filtered(selector: &Value) -> Vec<Self> {
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
