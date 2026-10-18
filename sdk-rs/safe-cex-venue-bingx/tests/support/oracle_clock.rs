use safe_cex_store::virtual_clock::VIRTUAL_CLOCK;
use serde_json::Value;

// Source mapper time differs from seeded orders (1234) and native wall time.
const SOURCE_MAPPER_TIME: f64 = 5678.0;

pub struct ClockWindow {
    before: f64,
    after: f64,
}

impl ClockWindow {
    pub fn start() -> Self {
        Self {
            before: VIRTUAL_CLOCK.get_current_time_ms(),
            after: f64::NAN,
        }
    }

    pub fn finish(mut self) -> Self {
        self.after = VIRTUAL_CLOCK.get_current_time_ms();
        self
    }

    pub fn normalize_orders(&self, native: &mut Value, expected: &Value) {
        let native = native.as_array_mut().unwrap();
        let expected = expected.as_array().unwrap();
        assert_eq!(
            native.len(),
            expected.len(),
            "BingX order observation count"
        );
        for (native, expected) in native.iter_mut().zip(expected) {
            if timestamp(&expected["lastUpdate"]) == SOURCE_MAPPER_TIME {
                let actual = timestamp(&native["lastUpdate"]);
                assert!(
                    (self.before..=self.after).contains(&actual),
                    "BingX mapped order timestamp {actual} outside shared clock [{}, {}]",
                    self.before,
                    self.after
                );
                native["lastUpdate"] = expected["lastUpdate"].clone();
            }
        }
    }
}

fn timestamp(value: &Value) -> f64 {
    if let Some(number) = value.as_f64() {
        return number;
    }
    let bits = value["number"].as_str().unwrap();
    f64::from_bits(u64::from_str_radix(bits, 16).unwrap())
}
