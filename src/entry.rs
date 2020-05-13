use crate::ipnet::InNet;
use crate::ipnet::ToMac;
use crate::ipnet::TryInNet;
use crate::ipnet::TryToMac;
use std::convert::TryInto;
use std::convert::{From, TryFrom};

use crate::hosts::Host;
use crate::network::InterfaceNetwork;
use globset::Glob;
use ipnetwork::IpNetwork;
use log::warn;
use pnet::datalink::{interfaces, MacAddr, NetworkInterface};
use serde_yaml::{Mapping, Value};
use std::io::{self, Write};
use std::iter;
use std::net::IpAddr;
use tabwriter::TabWriter;

pub struct Entry {
    pub name: String,
    pub mac: Option<MacAddr>,
    pub ip: IpAddr,
}

impl Entry {
    pub fn new(name: &str, mac: Option<MacAddr>, ip: IpAddr) -> Self {
        Entry {
            name: name.to_string(),
            mac,
            ip,
        }
    }

    pub fn as_dnsmasq(&self) -> String {
        todo!()
    }

    pub fn as_zone_entry(&self) -> String {
        todo!()
    }
}

pub fn entries_from_val(val: Value) -> impl Iterator<Item = Entry> {
    match val {
        Value::Sequence(seq) => entries_from_seq(seq),
        _ => entries_from_seq(vec![val]),
    }
}

fn entries_from_seq(seq: serde_yaml::Sequence) -> impl Iterator<Item = Entry> {
    seq.into_iter()
        .filter_map(|v| match v {
            Value::Mapping(map) => Some(entries_from_map(map)),
            _ => {
                warn!("invalid entry map: {:?}", v);
                None
            }
        })
        .flatten()
}

fn entries_from_map(map: Mapping) -> impl Iterator<Item = Entry> {
    map.into_iter().flat_map(|(k, v)| {
        let nets = InterfaceNetwork::filtered(&k);
        Host::new_hosts(v).flat_map(move |h| {
            nets.clone().into_iter().filter_map(move |net| {
                let ip = h.get_ip(&net)?;
                Some(Entry::new(&h.name, h.get_mac(&net), ip))
            })
        })
    })
}