use serde_json::{Value, json};

pub fn source_census(target: &str, value: &Value) {
    let mut cases = serde_json::Map::new();
    let mut outputs = serde_json::Map::new();
    let mut total = 0usize;
    for (name, corpus) in value.as_object().unwrap() {
        if let Some(rows) = corpus.as_array() {
            cases.insert(name.clone(), json!(rows.len()));
            total += rows.len();
            if rows
                .iter()
                .any(|row| row.get("result").is_some_and(Value::is_array))
            {
                outputs.insert(
                    name.clone(),
                    json!(
                        rows.iter()
                            .map(|row| row.get("result").and_then(Value::as_array).map(Vec::len))
                            .collect::<Vec<_>>()
                    ),
                );
            }
        }
    }
    let mut census = json!({"cases": cases, "totalCases": total, "outputs": outputs});
    if target == "reads" {
        let mut counts = serde_json::Map::new();
        let mut totals = serde_json::Map::new();
        for key in ["calls", "effects", "reports", "cache"] {
            let lengths: Vec<usize> = value["leverages"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row[key].as_array().unwrap().len())
                .collect();
            totals.insert(key.to_owned(), json!(lengths.iter().sum::<usize>()));
            counts.insert(key.to_owned(), json!(lengths));
        }
        census["leverageCounts"] = Value::Object(counts);
        census["leverageTotals"] = Value::Object(totals);
    }
    if target == "helpers" {
        census["idCalls"] = json!(
            value["ids"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["cases"].as_array().unwrap().len())
                .collect::<Vec<_>>()
        );
    }
    if target == "types" {
        let mut exports = serde_json::Map::new();
        let mut ws = serde_json::Map::new();
        for family in ["futures", "spot"] {
            let counts: serde_json::Map<String, Value> = value[family]
                .as_object()
                .unwrap()
                .iter()
                .map(|(name, rows)| (name.clone(), json!(rows.as_array().unwrap().len())))
                .collect();
            exports.insert(family.to_owned(), Value::Object(counts));
            let counts: serde_json::Map<String, Value> = value[family]["BASE_WS_URL"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    (
                        row[0].as_str().unwrap().to_owned(),
                        json!(row[1].as_array().unwrap().len()),
                    )
                })
                .collect();
            ws.insert(family.to_owned(), Value::Object(counts));
        }
        census["exports"] = Value::Object(exports);
        census["wsTables"] = Value::Object(ws);
        for key in ["probes", "nestedProbes"] {
            let mut families = serde_json::Map::new();
            for (family, tables) in value[key].as_object().unwrap() {
                let counts: serde_json::Map<String, Value> = tables
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(name, rows)| (name.clone(), json!(rows.as_array().unwrap().len())))
                    .collect();
                families.insert(family.clone(), Value::Object(counts));
            }
            census[key] = Value::Object(families);
        }
    }
    let expected: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
    assert_eq!(
        census, expected[target],
        "KRAKEN-CARDINALITY source total and per-case counts {target}"
    );
}

#[allow(dead_code)] // Helper outputs are scalar; other targets observe native collections.
pub fn native_len(actual: usize, expected: &Value, label: &str) {
    assert_eq!(
        Some(actual),
        expected.as_array().map(Vec::len),
        "KRAKEN-CARDINALITY independent native length {label}"
    );
}

// Bounds come from a committed independent census, never the live oracle.
pub fn cases<'a>(target: &str, name: &str, oracle: &'a Value) -> impl Iterator<Item = &'a Value> {
    let expected: Value = serde_json::from_str(include_str!("cardinality.json")).unwrap();
    let count = expected[target]["cases"][name].as_u64().unwrap() as usize;
    let rows = oracle[name].as_array().unwrap();
    (0..count).map(move |index| {
        rows.get(index)
            .expect("KRAKEN-CARDINALITY expected/native case index coverage")
    })
}
