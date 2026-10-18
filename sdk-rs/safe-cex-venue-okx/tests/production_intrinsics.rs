//! Regressions execute the production dispatcher included by the support adapter.
#[path = "support/production_intrinsics.rs"]
#[allow(dead_code)]
mod production;
use production::*;
use safe_cex_exchange::js_runtime::value::Js;

#[test]
fn spread_retains_every_group_and_native_references() {
    let receiver = object(vec![]);
    for name in ["array-spread", "array-spread-two"] {
        for count in [0, 1, 2, 5] {
            let rows = (0..count)
                .map(|i| object(vec![("id", n(i as f64))]))
                .collect::<Vec<_>>();
            let args = rows.iter().map(|v| array(vec![v.clone()])).collect();
            let result = intrinsic(receiver.clone(), name, args).unwrap();
            let actual = list(&result);
            assert_eq!(
                actual.len(),
                count,
                "production spread independent group floor"
            );
            for (i, (actual, original)) in actual.iter().zip(&rows).enumerate() {
                assert!(
                    actual.same_reference(original),
                    "production spread native object reference {i}"
                );
                assert_eq!(
                    numeric(&get(actual, "id")),
                    i as f64,
                    "production spread source group order"
                );
            }
        }
    }
}

#[test]
fn spread_uses_shared_iterators_and_rejects_invalid_later_groups() {
    let receiver = object(vec![]);
    let set = Js::new_set();
    set.call_method("add", vec![s("set")]).unwrap();
    let holey = Js::holey_array(vec![None, Some(n(-0.))]);
    let result = intrinsic(
        receiver.clone(),
        "array-spread",
        vec![
            array(vec![n(1.)]),
            set,
            Js::utf16(&[0xd83d, 0xde00, 0xd800]),
            holey,
        ],
    )
    .unwrap();
    let rows = list(&result);
    assert_eq!(rows.len(), 6, "production spread complete iterable count");
    assert_eq!(numeric(&rows[0]), 1.);
    assert_eq!(string(&rows[1]), "set");
    assert_eq!(&*rows[2].to_js_string().unwrap(), &[0xd83d, 0xde00]);
    assert_eq!(&*rows[3].to_js_string().unwrap(), &[0xd800]);
    assert!(
        rows[4].is_undefined(),
        "production spread materializes hole"
    );
    assert_eq!(numeric(&rows[5]).to_bits(), (-0f64).to_bits());
    let bad = array(vec![u()]);
    // A noniterable fifth argument must fail rather than being dropped.
    let final_group = object(vec![]);
    let thrown = intrinsic(
        receiver,
        "array-spread",
        vec![bad.clone(), bad.clone(), bad.clone(), bad, final_group],
    )
    .unwrap_err();
    assert_eq!(
        string(&get(&thrown.value, "name")),
        "TypeError",
        "production spread rejects invalid later iterable"
    );
}
