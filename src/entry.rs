use crate::hosts::Host;
use crate::network::InterfaceNetwork;
use crate::chain::ChainedEntryIterator;
use log::warn;
use pnet::datalink::MacAddr;
use serde_yaml::{Mapping, Value};
use std::io::{self, Write};
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

    pub fn as_dnsmasq_entry(&self) -> String {
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

    pub fn as_zone_entry(&self) -> String {
        let mut elems = vec![self.name.to_string()];
        if self.ip.is_ipv6() {
            elems.push("AAAA".to_string());
        } else {
            elems.push("A".to_string());
        }
        elems.push(self.ip.to_string());
        elems.join("\t")
    }

    pub fn as_env_var(&self) -> String {
        let v = if self.ip.is_ipv4() { "V4" } else { "V6" };
        format!(
            "{}_{}={}",
            self.name.replace('.', "_").replace('-', "_").to_uppercase(),
            v,
            self.ip
        )
    }
}

pub trait EntryIterator
where
    Self: Iterator<Item = Entry> + Sized,
{
    fn as_dnsmasq_reservations(self) -> FormattedEntries<Self> {
        FormattedEntries::DnsmasqReservations(self)
    }

    fn as_zone_records(self) -> FormattedEntries<Self> {
        FormattedEntries::ZoneRecords(self)
    }
    fn as_env_vars(self) -> FormattedEntries<Self> {
        FormattedEntries::EnvVars(self)
    }

    fn chain_entries<J: Iterator<Item=Entry> + Sized>(self, other: J) -> ChainedEntryIterator<Self, J> {
        ChainedEntryIterator::new(self, other)
    }
}

impl<I: Iterator<Item = Entry> + Sized> EntryIterator for I {}

pub enum FormattedEntries<I: Iterator<Item = Entry> + Sized> {
    DnsmasqReservations(I),
    ZoneRecords(I),
    EnvVars(I),
}

impl<I: Iterator<Item = Entry> + Sized> FormattedEntries<I> {
    pub fn write<W: io::Write>(self, w: &mut W) -> std::io::Result<()> {
        match self {
            Self::ZoneRecords(_) => {
                let mut w = TabWriter::new(w);
                self.raw_write(&mut w)?;
                w.flush()
            }
            Self::DnsmasqReservations(_) | Self::EnvVars(_) => self.raw_write(w),
        }
    }
    fn raw_write<W: io::Write>(self, w: &mut W) -> std::io::Result<()> {
        for s in self {
            writeln!(w, "{}", s)?;
        }
        Ok(())
    }
}

impl<I: Iterator<Item = Entry> + Sized> IntoIterator for FormattedEntries<I> {
    type Item = String;

    type IntoIter = std::iter::Map<I, fn(I::Item) -> String>;
    fn into_iter(self) -> Self::IntoIter {
        match self {
            Self::DnsmasqReservations(i) => i.map(|e| e.as_dnsmasq_entry()),
            Self::ZoneRecords(i) => i.map(|e| e.as_zone_entry()),
            Self::EnvVars(i) => i.map(|e| e.as_env_var()),
        }
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