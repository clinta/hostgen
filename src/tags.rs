use serde_yaml::Value;
use std::collections;
use std::collections::HashSet;
use std::iter::{once, FromIterator};

#[derive(Clone)]
pub struct Tags(HashSet<String>);

impl Tags {
    pub fn new(val: &Value) -> Self {
        if let Some(seq) = val.as_sequence() {
            seq.iter().cloned().collect()
        } else {
            once(val).cloned().collect()
        }
    }

    pub fn new_child(&self, val: &Value) -> Self {
        let mut r = self.clone();
        for v in Self::new(val) {
            if v.chars().nth(0).filter(|c| c == &'!').is_some() {
                r.0.remove(&v[1..]);
            } else {
                r.0.insert(v);
            }
        }
        r
    }

    pub fn contains(&self, v: &String) -> bool {
        self.0.contains(v)
    }
}

impl From<HashSet<String>> for Tags {
    fn from(h: HashSet<String>) -> Self {
        Self(h)
    }
}

impl FromIterator<String> for Tags {
    fn from_iter<I: IntoIterator<Item = String>>(iter: I) -> Self {
        iter.into_iter().collect::<HashSet<String>>().into()
    }
}

impl FromIterator<Value> for Tags {
    fn from_iter<I: IntoIterator<Item = Value>>(iter: I) -> Self {
        iter.into_iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect()
    }
}

impl IntoIterator for Tags {
    type Item = String;
    type IntoIter = collections::hash_set::IntoIter<String>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}