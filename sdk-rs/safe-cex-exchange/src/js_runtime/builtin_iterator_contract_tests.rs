use super::*;
use serde_json::json;

#[test]
fn accepted_iterator_cached_member_retains_live_method() {
    let expected = super::super::collections::accepted_contract_tests::expectations();
    for row in expected["cache"].as_array().unwrap() {
        let owner = if row["kind"] == "Map" {
            let owner = Js::new_map();
            owner
                .call_method("set", vec![Js::str("k"), Js::Number(13.5)])
                .unwrap();
            owner
        } else {
            let owner = Js::new_set();
            owner.call_method("add", vec![Js::Number(17.5)]).unwrap();
            owner
        };
        let method = if row["method"] == "values" {
            "values"
        } else {
            "entries"
        };
        let original = owner.get(method).unwrap();
        let fresh = Js::function("unused replacement", |_, _| Ok(Js::Null));
        let cached = cached_iterator_member(&owner, method, fresh);
        let same = cached.same_reference(&original);
        let value = cached
            .call(owner.clone(), vec![])
            .and_then(|iterator| iterator.call_method("next", vec![]))
            .and_then(|step| step.get("value"));
        let value = match value {
            Ok(value) => super::super::value::to_json(&value).unwrap(),
            Err(error) => json!({"error":error.value.get("name").unwrap().to_display()}),
        };
        assert_eq!(
            json!({"kind":row["kind"],"method":method,"same":same,"value":value}),
            *row,
            "cached_iterator_member native identity and invocation"
        );
    }
}
