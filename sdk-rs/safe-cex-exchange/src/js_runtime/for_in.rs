//! Enumerable string-key iteration over the canonical native property graph.
use super::value::{Js, Str, Thrown};
use std::collections::{HashSet, VecDeque};

pub struct ForInRecord {
    current: Option<Js>,
    remaining: VecDeque<Str>,
    visited: HashSet<Str>,
    entered: bool,
}
impl ForInRecord {
    pub fn get(value: &Js) -> Self {
        Self {
            current: (!value.is_nullish()).then(|| value.clone()),
            remaining: VecDeque::new(),
            visited: HashSet::new(),
            entered: false,
        }
    }
    pub fn next_key(&mut self) -> Result<Option<Str>, Thrown> {
        while let Some(current) = self.current.clone() {
            if !self.entered {
                self.remaining = current.for_in_own_names()?.into();
                self.entered = true;
            }
            while let Some(key) = self.remaining.pop_front() {
                if self.visited.contains(&key) {
                    continue;
                }
                if let Some(enumerable) = current.own_enumerability(&key)? {
                    self.visited.insert(key.clone());
                    if enumerable {
                        return Ok(Some(key));
                    }
                }
            }
            self.current = current.prototype();
            self.entered = false;
        }
        Ok(None)
    }
}
