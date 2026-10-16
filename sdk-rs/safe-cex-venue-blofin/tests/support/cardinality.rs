//! Reviewed, frozen expectations, independent of the live oracle and native outputs.
//! Never refresh this pin merely because a test failed.
use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;

pub struct Census {
    name: &'static str,
    pin: Value,
    hits: RefCell<BTreeMap<&'static str, usize>>,
    compared: RefCell<BTreeMap<(usize, usize), usize>>,
}
#[allow(dead_code)] // Shared by scalar and collection targets.
impl Census {
    pub fn new(name: &'static str, corpus: &Value) -> Self {
        let pins: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
        let this = Self {
            name,
            pin: pins[name].clone(),
            hits: RefCell::default(),
            compared: RefCell::default(),
        };
        for (path, count) in this.pin["arrays"].as_object().unwrap() {
            assert_eq!(
                corpus.pointer(path).and_then(Value::as_array).map(Vec::len),
                Some(count.as_u64().unwrap() as usize),
                "BLOFIN-CARDINALITY source count target={name} path={path}"
            );
        }
        for (path, expected) in this.pin["selectors"].as_object().unwrap() {
            assert_eq!(
                &corpus.pointer(path).cloned().unwrap_or(Value::Null),
                expected,
                "BLOFIN-CARDINALITY source branch target={name} path={path}"
            );
        }
        this
    }
    pub fn count(&self, path: &str) -> usize {
        self.pin["arrays"][path]
            .as_u64()
            .expect("pinned collection") as usize
    }
    pub fn case_count(&self, index: usize, suffix: &str) -> usize {
        self.count(&format!("/cases/{index}{suffix}"))
    }
    pub fn native(&self, index: usize, suffix: &str, length: usize) {
        assert_eq!(
            length,
            self.case_count(index, suffix),
            "BLOFIN-CARDINALITY native length target={} case={index} field={suffix}",
            self.name
        );
    }
    pub fn compared(&self, case: usize, path: usize) {
        *self.compared.borrow_mut().entry((case, path)).or_default() += 1;
        self.hit("comparisons");
    }
    pub fn finish_comparisons(&self, paths: usize) {
        self.finish("comparisons", self.count("/cases") * paths);
        for case in 0..self.count("/cases") {
            for path in 0..paths {
                assert_eq!(
                    self.compared.borrow().get(&(case, path)).copied(),
                    Some(1),
                    "BLOFIN-CARDINALITY executed comparisons index target={} case={case} path={path}",
                    self.name
                );
            }
        }
    }
    pub fn hit(&self, label: &'static str) {
        *self.hits.borrow_mut().entry(label).or_default() += 1;
    }
    pub fn finish(&self, label: &'static str, expected: usize) {
        assert_eq!(
            self.hits.borrow().get(label).copied().unwrap_or(0),
            expected,
            "BLOFIN-CARDINALITY executed comparisons target={} loop={label}",
            self.name
        );
    }
}
