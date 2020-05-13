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
use std::iter;
use std::net::IpAddr;
use tabwriter::TabWriter;
use crate::network::InterfaceNetwork;

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

pub trait AsEntries {
    fn as_entries(&self) -> Box<dyn Iterator<Item = Entry> + '_>;
}

impl AsEntries for Value {
    fn as_entries(&self) -> Box<dyn Iterator<Item = Entry> + '_> {
        match self {
            Value::Sequence(seq) => Box::new(seq.iter().map(|v| v.as_entries()).flatten()),
            Value::Mapping(map) => map.as_entries(),
            _ => {
                warn!("unable to convert to entries: {:?}", self);
                Box::new(iter::empty())
            }
        }
    }
}

impl AsEntries for Mapping {
    fn as_entries(&self) -> Box<dyn Iterator<Item = Entry> + '_> {
        self.iter().map(|(k, v)|  {
            let net = InterfaceNetwork::filtered(k);
            ""
        }
    );
        todo!()
    }
}
