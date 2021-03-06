use globset::Glob;
use ipnetwork::IpNetwork;
use pnet::datalink::{interfaces, NetworkInterface};
use serde_yaml::Value;
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceNetwork {
    pub iface: Option<NetworkInterface>,
    pub network: IpNetwork,
}

impl InterfaceNetwork {
    fn new_with_interface(iface: NetworkInterface, network: IpNetwork) -> Self {
        Self {
            iface: Some(iface),
            network,
        }
    }

    fn new_net_only(network: IpNetwork) -> Self {
        Self {
            iface: None,
            network,
        }
    }
    fn none_v4() -> Self {
        Self::new_net_only("0.0.0.0/0".parse().unwrap())
    }

    fn none_v6() -> Self {
        Self::new_net_only("::/0".parse().unwrap())
    }

    fn all() -> Vec<Self> {
        interfaces()
            .iter()
            .map(|i| {
                i.ips
                    .iter()
                    .map(move |net| Self::new_with_interface(i.clone(), *net))
            })
            .flatten()
            .collect()
    }

    pub fn filtered(selector: &Value) -> Vec<Self> {
        Self::filter_networks(&Self::all(), selector)
    }

    fn filter_networks(networks: &[Self], selector: &Value) -> Vec<Self> {
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

        if selector.is_null() {
            return vec![Self::none_v4(), Self::none_v6()];
        }

        if let Some(i) = selector.as_u64().and_then(|x| u32::try_from(x).ok()) {
            return networks
                .iter()
                .filter(|x| x.iface.as_ref().filter(|iface| iface.index == i).is_some())
                .cloned()
                .collect();
        }

        if let Some(s) = selector.as_str() {
            if s.starts_with('!') {
                let exclude_selector = Value::String(s[1..].to_string());
                let excludes = &Self::filter_networks(networks, &exclude_selector);
                return networks
                    .iter()
                    .filter(|n| !excludes.contains(n))
                    .cloned()
                    .collect();
            }

            match s.to_lowercase().as_ref() {
                "v4" | "ip4" | "ipv4" => {
                    return networks
                        .iter()
                        .filter(|x| x.network.is_ipv4())
                        .cloned()
                        .collect()
                }
                "v6" | "ip6" | "ipv6" => {
                    return networks
                        .iter()
                        .filter(|x| x.network.is_ipv6())
                        .cloned()
                        .collect()
                }
                _ => {}
            };

            if let Ok(net) = s.parse::<IpNetwork>() {
                return networks
                    .iter()
                    .filter(|x| net.contains(x.network.ip()))
                    .cloned()
                    .collect();
            }

            if let Ok(glob) = Glob::new(s) {
                let glob = glob.compile_matcher();
                return networks
                    .iter()
                    .filter(|x| {
                        x.iface
                            .as_ref()
                            .filter(|iface| glob.is_match(&iface.name))
                            .is_some()
                    })
                    .cloned()
                    .collect();
            }

            return networks
                .iter()
                .filter(|x| x.iface.as_ref().filter(|iface| iface.name == s).is_some())
                .cloned()
                .collect();
        }

        Vec::new()
    }
}
