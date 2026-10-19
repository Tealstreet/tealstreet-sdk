use safe_cex_exchange::js_runtime::value::Js;
use serde_json::Value;
use std::process::Command;
mod observation {
    use safe_cex_exchange::js_runtime::globals::install_builtins;
    use safe_cex_exchange::js_runtime::value::{Js, Thrown, WriteObserver};
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/js-entrypoint-observers.rs"
    ));
}
#[test]
fn missing_js_entrypoints_match_native_and_node_observations() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/js-entrypoint-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let profiles: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for row in profiles {
        let method = row["method"].as_str().unwrap();
        let input = &row["input"];
        let actual = match method {
            "sort_array_with" => observation::sort(input, |target, compare| {
                Js::sort_array_with(target, compare)
            }),
            "splice_array" => observation::splice(input, Js::splice_array),
            "ordinary_has_instance" => observation::instance(input, Js::ordinary_has_instance),
            "strict_equals" => observation::equality(input, Js::strict_equals),
            "same_value_zero" => observation::equality(input, Js::same_value_zero),
            "same_reference" => observation::equality(input, Js::same_reference),
            "set_write_observer" => observation::observer(input, Js::set_write_observer),
            "set_key" => observation::property(input, Js::set_key),
            "define_key" => observation::property(input, Js::define_key),
            "push" => observation::push(input, Js::push),
            "array" => observation::array(input, Js::array),
            "holey_array" => observation::holey(input, Js::holey_array),
            "call" => observation::call(input, Js::call),
            _ => panic!("unreviewed entrypoint"),
        };
        assert_eq!(actual, row["output"], "JS_ENTRYPOINT {method} {input}");
    }
}
