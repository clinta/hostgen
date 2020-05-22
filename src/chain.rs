use crate::entry::Entry;
use pnet::datalink::MacAddr;
use std::collections::HashSet;
use std::net::IpAddr;

struct EntryHash(HashSet<String>, HashSet<MacAddr>, HashSet<IpAddr>);

impl EntryHash {
    fn new() -> Self {
        Self(HashSet::new(), HashSet::new(), HashSet::new())
    }
    fn insert(&mut self, e: &Entry) {
        self.0.insert(e.name.clone());
        if let Some(mac) = e.mac {
            self.1.insert(mac);
        }
        self.2.insert(e.ip);
    }

    fn contains(&self, e: &Entry) -> bool {
        self.0.contains(&e.name)
            || e.mac.map_or(false, |mac| self.1.contains(&mac))
            || self.2.contains(&e.ip)
    }

    fn fill_from(&mut self, other: &mut EntryHash) {
        for name in other.0.drain() {
            self.0.insert(name);
        }
        for mac in other.1.drain() {
            self.1.insert(mac);
        }
        for ip in other.2.drain() {
            self.2.insert(ip);
        }
    }
}

pub struct ChainedEntryIterator<
    I: Iterator<Item = Entry> + Sized,
    J: Iterator<Item = Entry> + Sized,
> {
    first: I,
    other: J,
    first_hashes: EntryHash,
}

impl<I: Iterator<Item = Entry> + Sized, J: Iterator<Item = Entry> + Sized>
    ChainedEntryIterator<I, J>
{
    pub fn new(first: I, other: J) -> Self {
        Self {
            first,
            other,
            first_hashes: EntryHash::new(),
        }
    }
}

impl<I: Iterator<Item = Entry> + Sized, J: Iterator<Item = Entry> + Sized> Iterator
    for ChainedEntryIterator<I, J>
{
    type Item = Entry;
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.first.next() {
            self.first_hashes.insert(&next);
            return Some(next);
        }

        if let Some(next) = self.other.next() {
            if !self.first_hashes.contains(&next) {
                return Some(next);
            } else {
                return self.next();
            }
        }
        None
    }
}

pub struct FlatEntryIterator<I: Iterator<Item = Entry> + Sized, II: Iterator<Item = I>> {
    iter: II,
    curent_iter: Option<II::Item>,
    current_hashes: EntryHash,
    previous_hashes: EntryHash,
}

impl<I: Iterator<Item = Entry> + Sized, II: Iterator<Item = I>> FlatEntryIterator<I, II> {
    pub fn new(mut iter: II) -> Self {
        Self {
            curent_iter: iter.next(),
            iter,
            current_hashes: EntryHash::new(),
            previous_hashes: EntryHash::new(),
        }
    }
}

impl<I: Iterator<Item = Entry> + Sized, II: Iterator<Item = I>> Iterator
    for FlatEntryIterator<I, II>
{
    type Item = Entry;
    fn next(&mut self) -> Option<Self::Item> {
        match self.curent_iter.as_mut() {
            None => None,
            Some(iter) => {
                if let Some(next) = iter.next() {
                    if self.previous_hashes.contains(&next) {
                        self.next()
                    } else {
                        self.current_hashes.insert(&next);
                        Some(next)
                    }
                } else {
                    self.previous_hashes.fill_from(&mut self.current_hashes);
                    self.curent_iter = self.iter.next();
                    self.next()
                }
            }
        }
    }
}

pub trait IntoFlatEntryIterator<I: Iterator<Item = Entry> + Sized>
where
    Self: Iterator<Item=I> + Sized,
{
    fn flatten_entries(self) -> FlatEntryIterator<I, Self> {
        FlatEntryIterator::new(self)
    }
}

impl<I: Iterator<Item = Entry> + Sized, II: Iterator<Item = I>> IntoFlatEntryIterator<I> for II {}