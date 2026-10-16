use safe_cex_num::ProtoMember;
use safe_cex_store::instrument_class::{
    AssetClass, InstrumentClassResult, bybit_instrument_class, describe_instrument,
};
use safe_cex_types::Market;
use serde_json::{Value, json};
#[test]
fn inherited_instrument_members_keep_native_identity_tags() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/instrument_findings_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "source native instrument oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 12, "independent inherited member floor");
    let members = [
        ProtoMember::Constructor,
        ProtoMember::DefineGetter,
        ProtoMember::DefineSetter,
        ProtoMember::HasOwnProperty,
        ProtoMember::LookupGetter,
        ProtoMember::LookupSetter,
        ProtoMember::IsPrototypeOf,
        ProtoMember::PropertyIsEnumerable,
        ProtoMember::ToString,
        ProtoMember::ValueOf,
        ProtoMember::Prototype,
        ProtoMember::ToLocaleString,
    ];
    let keys = [
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
    ];
    let mut seen = Vec::new();
    for (i, (key, member)) in keys.into_iter().zip(members).enumerate() {
        let market:Market=serde_json::from_value(json!({"id":"X","symbol":"X","base":"X","quote":"USD","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":10},"leverage":{"min":1,"max":10}},"info":{"symbolType":key}})).unwrap();
        let native = bybit_instrument_class(&market);
        let InstrumentClassResult::Classified {
            asset_class,
            ref venue_label,
            ..
        } = native
        else {
            panic!("source inherited class remains classified")
        };
        assert_eq!(
            asset_class,
            AssetClass::PrototypeProperty(member),
            "INSTRUMENT-NATIVE-MEMBER {key}"
        );
        assert_eq!(venue_label, key, "native label retained");
        assert_eq!(
            bybit_instrument_class(&market),
            native,
            "native tag identity stable across calls"
        );
        assert!(
            !seen.contains(&asset_class),
            "native tags distinguish all twelve source referents"
        );
        seen.push(asset_class);
        assert_eq!(rows[i]["key"], key);
        assert_eq!(
            member.length().map(u64::from),
            rows[i]["arity"].as_u64(),
            "native arity/kind survives"
        );
        assert_eq!(
            rows[i]["kind"],
            if member == ProtoMember::Prototype {
                "object"
            } else {
                "function"
            }
        );
        assert_eq!(
            describe_instrument("X", "bybit", &native),
            "X on bybit: undefined — may require signing a trading agreement",
            "downstream source noun still undefined"
        );
    }
    assert_eq!(
        seen.len(),
        12,
        "stop toLocaleString; tag identity is not executable JS capability parity"
    );
}
