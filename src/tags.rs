use std::collections::HashSet;
use serde_yaml::{Value, Sequence};

pub struct Tags(HashSet<String>);

impl Tags {
    pub fn new(val: &Value) -> Self {
        if let Some(seq) = val.as_sequence() {
            return Self(
                seq.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect()
            )
        } else  {
            
        }
    }

    pub fn new_child(&self, val: &Value) -> Self {
        Self(self.0.clone().union(Self::new(val)).into())
        //todo!()
    }
}