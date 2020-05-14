use serde_yaml::Value;
use std::collections;
use std::collections::HashSet;
use std::iter::{once, FromIterator};

#[derive(Clone, Debug)]
pub struct Tags(HashSet<String>);

impl Tags {
    pub fn new() -> Self {
        Self(HashSet::new())
    }

    pub fn extract(&self, val: &Value) -> Self {
        match val {
            Value::Sequence(seq) => seq
                .iter()
                .filter(|v| v.is_mapping())
                .fold(self.clone(), |t, m| t.extract(m)),
            Value::Mapping(map) => map
                .iter()
                .filter(|(k, _)| k.as_str().filter(|k| k.starts_with("_tag")).is_some())
                .map(|(_, v)| v)
                .fold(self.clone(), |t, v| t.new_child(v)),
            _ => self.clone(),
        }
    }

    fn from_val(val: &Value) -> Self {
        if let Some(seq) = val.as_sequence() {
            seq.iter().cloned().collect()
        } else {
            once(val).cloned().collect()
        }
    }

    pub fn new_child(&self, val: &Value) -> Self {
        let mut r = self.clone();
        for v in Self::from_val(val) {
            if v.starts_with("!") {
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
