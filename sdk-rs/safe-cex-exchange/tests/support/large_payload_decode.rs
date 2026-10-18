use safe_cex_exchange::js_runtime::value::{
    from_retained_input, from_typed, to_retained_input, to_typed,
};
use safe_cex_types::{Market, retained_json::RetainedJsonInput as Input};
use serde::Deserialize;
use std::{
    hash::{Hash, Hasher},
    time::Instant,
};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Profile {
    name: String,
    count: usize,
    markets_file: String,
    raw_files: Vec<String>,
    source_cpu_ms: f64,
    source_wall_ms: f64,
}

fn decode(value: Input) -> Input {
    let Input::Array(mut fields) = value else {
        panic!("tagged fixture")
    };
    let payload = fields.pop().unwrap();
    let tag = if fields.is_empty() {
        &payload
    } else {
        &fields[0]
    };
    match tag
        .as_text()
        .map(String::from_utf16_lossy)
        .unwrap()
        .as_str()
    {
        "u" => Input::Undefined,
        "z" => Input::Null,
        "s" | "b" => payload,
        "f" => Input::Float(f64::from_bits(
            u64::from_str_radix(&String::from_utf16_lossy(payload.as_text().unwrap()), 16).unwrap(),
        )),
        "a" => {
            let Input::Array(values) = payload else {
                panic!("array fixture")
            };
            Input::Array(values.into_iter().map(decode).collect())
        }
        "o" => {
            let Input::Array(values) = payload else {
                panic!("object fixture")
            };
            Input::Object(
                values
                    .into_iter()
                    .map(|value| {
                        let Input::Array(mut pair) = value else {
                            panic!("entry fixture")
                        };
                        let value = decode(pair.pop().unwrap());
                        let Input::Text(key) = pair.pop().unwrap() else {
                            panic!("key fixture")
                        };
                        (key, value)
                    })
                    .collect(),
            )
        }
        tag => panic!("unknown fixture tag {tag}"),
    }
}

fn encode(value: Input) -> Input {
    let (tag, payload) = match value {
        Input::Undefined => ("u", Input::Null),
        Input::Null => ("z", Input::Null),
        Input::Bool(v) => ("b", Input::Bool(v)),
        Input::Text(v) => ("s", Input::Text(v)),
        Input::Lexeme(v) => ("l", Input::Text(v.encode_utf16().collect())),
        Input::Float(v) => (
            "f",
            Input::Text(format!("{:016x}", v.to_bits()).encode_utf16().collect()),
        ),
        Input::Array(v) => ("a", Input::Array(v.into_iter().map(encode).collect())),
        Input::Object(v) => (
            "o",
            Input::Array(
                v.into_iter()
                    .map(|(k, v)| Input::Array(vec![Input::Text(k), encode(v)]))
                    .collect(),
            ),
        ),
    };
    Input::Array(vec![Input::Text(tag.encode_utf16().collect()), payload])
}

fn fingerprint(value: &Input, hash: &mut impl Hasher) {
    std::mem::discriminant(value).hash(hash);
    match value {
        Input::Undefined | Input::Null => {}
        Input::Bool(v) => v.hash(hash),
        Input::Text(v) => v.hash(hash),
        Input::Lexeme(v) => v.hash(hash),
        Input::Float(v) => v.to_bits().hash(hash),
        Input::Array(v) => {
            v.len().hash(hash);
            for child in v {
                fingerprint(child, hash);
            }
        }
        Input::Object(v) => {
            v.len().hash(hash);
            for (key, child) in v {
                key.hash(hash);
                fingerprint(child, hash);
            }
        }
    }
}

fn cpu_ms() -> f64 {
    unsafe extern "C" {
        fn clock() -> std::os::raw::c_long;
    }
    let ticks = unsafe { clock() };
    assert!(ticks >= 0, "process CPU clock unavailable");
    ticks as f64 / 1000.0
}

pub(super) fn recorded() {
    let manifest = std::env::var("LARGE_PAYLOAD_MANIFEST").expect("recorded fixture manifest");
    let output = std::env::var("LARGE_PAYLOAD_OUTPUT").expect("observation output prefix");
    let profiles: Vec<Profile> = serde_json::from_slice(&std::fs::read(manifest).unwrap()).unwrap();
    let mut over_budget = Vec::new();
    for profile in profiles {
        assert!(profile.source_cpu_ms.is_finite() && profile.source_cpu_ms > 0.0);
        assert!(profile.source_wall_ms.is_finite() && profile.source_wall_ms > 0.0);
        let raw: Vec<Vec<u16>> = profile
            .raw_files
            .iter()
            .map(|path| {
                std::fs::read_to_string(path)
                    .unwrap()
                    .encode_utf16()
                    .collect()
            })
            .collect();
        let cpu = cpu_ms();
        let start = Instant::now();
        let parsed: Vec<Input> = raw
            .iter()
            .map(|text| Input::parse_json(text).unwrap())
            .collect();
        let parse_ms = start.elapsed().as_secs_f64() * 1000.0;
        let parse_cpu_ms = cpu_ms() - cpu;
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        for value in &parsed {
            fingerprint(value, &mut hash);
        }
        let raw_hash = hash.finish();
        drop(parsed);
        let text: Vec<u16> = std::fs::read_to_string(&profile.markets_file)
            .unwrap()
            .encode_utf16()
            .collect();
        let native = from_retained_input(decode(Input::parse_json(&text).unwrap()));
        eprintln!("PAYLOAD_PHASE {} decode", profile.name);
        let cpu = cpu_ms();
        let start = Instant::now();
        let markets: Vec<Market> = to_typed(&native).unwrap();
        let decode_ms = start.elapsed().as_secs_f64() * 1000.0;
        let decode_cpu_ms = cpu_ms() - cpu;
        assert_eq!(markets.len(), profile.count);
        eprintln!("PAYLOAD_PHASE {} capture", profile.name);
        let cpu = cpu_ms();
        let start = Instant::now();
        let captured = to_retained_input(&native).unwrap();
        let capture_ms = start.elapsed().as_secs_f64() * 1000.0;
        let capture_cpu_ms = cpu_ms() - cpu;
        eprintln!("PAYLOAD_PHASE {} retained-decode", profile.name);
        let cpu = cpu_ms();
        let start = Instant::now();
        let decoded: Vec<Market> = captured.decode_native(false).unwrap();
        let native_decode_ms = start.elapsed().as_secs_f64() * 1000.0;
        let native_decode_cpu_ms = cpu_ms() - cpu;
        assert_eq!(decoded.len(), profile.count);
        eprintln!("PAYLOAD_PHASE {} observation", profile.name);
        let observation = encode(to_retained_input(&from_typed(&markets).unwrap()).unwrap())
            .json_stringify()
            .unwrap()
            .unwrap();
        let decomposed = encode(to_retained_input(&from_typed(&decoded).unwrap()).unwrap())
            .json_stringify()
            .unwrap()
            .unwrap();
        assert_eq!(decomposed, observation, "complete decomposed native output");
        let cpu_ratio = (parse_cpu_ms + decode_cpu_ms) / profile.source_cpu_ms;
        let wall_ratio = (parse_ms + decode_ms) / profile.source_wall_ms;
        std::fs::write(
            format!("{output}.{}.native.json", profile.name),
            observation,
        )
        .unwrap();
        eprintln!(
            "PAYLOAD_RESULT {}",
            serde_json::json!({"venue":profile.name,"count":markets.len(),"rawBytes":profile.raw_files.iter().map(|p|std::fs::metadata(p).unwrap().len()).sum::<u64>(),"parseMs":parse_ms,"decodeMs":decode_ms,"parseCpuMs":parse_cpu_ms,"decodeCpuMs":decode_cpu_ms,"captureMs":capture_ms,"captureCpuMs":capture_cpu_ms,"nativeDecodeMs":native_decode_ms,"nativeDecodeCpuMs":native_decode_cpu_ms,"sourceCpuMs":profile.source_cpu_ms,"sourceWallMs":profile.source_wall_ms,"cpuRatio":cpu_ratio,"wallRatio":wall_ratio,"debugAssertions":cfg!(debug_assertions),"rawHash":format!("{raw_hash:016x}")})
        );
        let payload_cpu_ms = parse_cpu_ms + decode_cpu_ms;
        if !cfg!(debug_assertions) && payload_cpu_ms > 250.0 {
            over_budget.push(format!("{}: {payload_cpu_ms:.3} ms CPU", profile.name));
        }
    }
    assert!(
        over_budget.is_empty(),
        "release bootstrap payload exceeds 250 ms CPU budget: {over_budget:?}"
    );
}
