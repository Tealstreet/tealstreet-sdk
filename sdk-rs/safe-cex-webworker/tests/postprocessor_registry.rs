use safe_cex_webworker::postprocessor_registry::{ImportedPostprocessors, PostprocessorRegistry};
use serde::Deserialize;
use std::rc::Rc;

type Factory = dyn Fn(f64) -> Processor;
struct Processor {
    name: &'static str,
    value: f64,
}
fn factory(name: &'static str) -> Rc<Factory> {
    Rc::new(move |value| Processor { name, value })
}
#[derive(Deserialize)]
struct Profile {
    rows: Vec<Row>,
    #[serde(rename = "listIdentity")]
    list_identity: bool,
}
#[derive(Deserialize)]
struct Row {
    label: String,
    independent: Vec<String>,
    dependent: Vec<String>,
    constructed: Vec<(String, f64)>,
}
#[test]
fn constructor_lists_keep_order_identity_and_live_mutation() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/postprocessor_registry_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Profile = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source.rows.len(),
        9,
        "REGISTRY source fixed operation count"
    );
    let names = [
        "DisplayedPositionsPostprocessor",
        "DisplayedMarginPostprocessor",
        "ChaserOrderEnrichmentPostprocessor",
        "OrdersRiskProfilePostprocessor",
        "ChaserOrderPostprocessor",
        "TwapPostprocessor",
        "MessageHandlerStatsPostprocessor",
    ];
    let builtins: Vec<_> = names.into_iter().map(factory).collect();
    let registry = PostprocessorRegistry::new(ImportedPostprocessors {
        displayed_positions: builtins[0].clone(),
        displayed_margin: builtins[1].clone(),
        chaser_order_enrichment: builtins[2].clone(),
        orders_risk_profile: builtins[3].clone(),
        chaser_order: builtins[4].clone(),
        twap: builtins[5].clone(),
        message_handler_stats: builtins[6].clone(),
    });
    let held = registry.independent_postprocessors.clone();
    let extension = factory("extension");
    let twin = factory("extension");
    let mut identities = builtins.clone();
    identities.push(extension.clone());
    identities.push(twin.clone());
    let expected: [&[usize]; 9] = [
        &[0, 1, 2, 3],
        &[0, 1, 2, 3],
        &[0, 1, 2, 3, 7],
        &[0, 1, 2, 3, 7],
        &[0, 1, 2, 3, 7, 8],
        &[0, 2, 3, 7, 8],
        &[0, 2, 3, 7, 8, 1],
        &[1, 8, 7, 3, 2, 0],
        &[1, 8, 7, 3, 2, 0],
    ];
    let labels = [
        "initial",
        "builtin-duplicate",
        "append",
        "same-extension",
        "same-name-distinct",
        "external-remove",
        "reappend-builtin",
        "external-reorder-and-duplicate",
        "dependent-reorder",
    ];
    for step in 0..9 {
        match step {
            1 => registry.register_independent_postprocessor(builtins[1].clone()),
            2 | 3 => registry.register_independent_postprocessor(extension.clone()),
            4 => registry.register_independent_postprocessor(twin.clone()),
            5 => {
                held.borrow_mut().remove(1);
            }
            6 => registry.register_independent_postprocessor(builtins[1].clone()),
            7 => {
                held.borrow_mut().reverse();
                registry.register_independent_postprocessor(extension.clone());
            }
            8 => registry
                .exchange_dependent_postprocessors
                .borrow_mut()
                .reverse(),
            _ => {}
        }
        let classes = held.borrow();
        // Assert decoded native constructor results and identities before source profiles.
        assert_eq!(
            classes.len(),
            expected[step].len(),
            "REGISTRY native cardinality step {step}"
        );
        for (index, class) in classes.iter().enumerate() {
            let identity = expected[step][index];
            assert!(
                Rc::ptr_eq(class, &identities[identity]),
                "REGISTRY native constructor identity step {step}/{index}"
            );
            let native = class(1.25);
            assert_eq!(
                native.value.to_bits(),
                1.25_f64.to_bits(),
                "REGISTRY native constructed value"
            );
            assert_eq!(
                native.name,
                if identity < 7 {
                    names[identity]
                } else {
                    "extension"
                },
                "REGISTRY native constructed name"
            );
        }
        let other = registry.exchange_dependent_postprocessors.borrow();
        assert_eq!(other.len(), 3, "REGISTRY native dependent count");
        let dependent_ids = if step == 8 { [6, 5, 4] } else { [4, 5, 6] };
        for index in 0..3 {
            assert!(
                Rc::ptr_eq(&other[index], &builtins[dependent_ids[index]]),
                "REGISTRY native dependent order {index}"
            );
            assert_eq!(
                other[index](7.5).value.to_bits(),
                7.5_f64.to_bits(),
                "REGISTRY native dependent constructed value"
            );
        }
        assert!(
            Rc::ptr_eq(&held, &registry.independent_postprocessors),
            "REGISTRY native list identity"
        );
        let row = &source.rows[step];
        assert_eq!(row.label, labels[step]);
        let keys: Vec<_> = expected[step]
            .iter()
            .map(|&i| {
                if i < 7 {
                    names[i]
                } else if i == 7 {
                    "extension"
                } else {
                    "twin"
                }
            })
            .collect();
        assert_eq!(
            row.independent, keys,
            "REGISTRY source/native identity order"
        );
        assert_eq!(
            row.dependent,
            dependent_ids.map(|i| names[i]),
            "REGISTRY source/native dependent order"
        );
        assert_eq!(
            row.constructed.len(),
            expected[step].len(),
            "REGISTRY source constructed cardinality step {step}"
        );
        for (index, class) in classes.iter().enumerate() {
            let native = class(1.25);
            assert_eq!(row.constructed[index].0, native.name);
            assert_eq!(row.constructed[index].1.to_bits(), native.value.to_bits());
        }
    }
    assert!(source.list_identity);
}
