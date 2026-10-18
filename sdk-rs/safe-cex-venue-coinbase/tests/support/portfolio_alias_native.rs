use crate::portfolios_balances::{
    Balance, Currencies, Portfolio, PortfolioBalance, PortfolioCurrencies, PortfolioFailure,
    PortfolioState, Portfolios, SupportedTypes,
};
use safe_cex_exchange::js_runtime::value::{Js, Thrown, to_typed, utf16};
use safe_cex_types::{
    Balance as NativeBalance, CurrencyBalance, retained_json::RetainedJsonInput as Input,
};
use std::{cell::RefCell, rc::Rc};

fn opaque() -> Js {
    Js::array(vec![
        Js::Number(-0.0),
        Js::Number(f64::NAN),
        Js::Number(f64::INFINITY),
        Js::Undefined,
        Js::String(vec![0xd800].into()),
    ])
}

fn assert_opaque(value: &Input) {
    let Input::Array(values) = value else {
        panic!("opaque array changed")
    };
    assert_eq!(values.len(), 5);
    assert!(matches!(values[0], Input::Float(value) if value.to_bits() == (-0.0_f64).to_bits()));
    assert!(matches!(values[1], Input::Float(value) if value.is_nan()));
    assert!(matches!(values[2], Input::Float(value) if value == f64::INFINITY));
    assert!(matches!(values[3], Input::Undefined));
    assert!(matches!(&values[4], Input::Text(units) if units.as_slice() == [0xd800]));
}

#[test]
fn portfolio_and_supported_type_aliases_keep_live_roots_and_native_metadata() {
    let mut rows = Vec::new();
    for uuid in ["first", "second"] {
        let input = Js::object(vec![
            ("uuid", Js::String(utf16(uuid))),
            ("type", Js::String(utf16("DEFAULT"))),
            ("missing", Js::Undefined),
            ("opaque", opaque()),
        ]);
        let parsed: PortfolioState = to_typed(&input).unwrap();
        let row: Portfolio = Rc::new(RefCell::new(parsed));
        assert_eq!(row.borrow().uuid, uuid);
        assert_eq!(row.borrow().r#type, "DEFAULT");
        assert_eq!(row.borrow().metadata.len(), 2);
        assert!(matches!(
            row.borrow()
                .metadata
                .get(&"missing".encode_utf16().collect::<Vec<_>>()),
            Some(Input::Undefined)
        ));
        assert_opaque(
            row.borrow()
                .metadata
                .get(&"opaque".encode_utf16().collect::<Vec<_>>())
                .unwrap(),
        );
        rows.push(row);
    }
    let portfolios: Portfolios = Rc::new(RefCell::new(rows.clone()));
    let captured = portfolios.clone();
    assert!(Rc::ptr_eq(&captured, &portfolios));
    assert_eq!(captured.borrow().len(), 2);
    assert!(Rc::ptr_eq(&captured.borrow()[0], &rows[0]));
    assert!(Rc::ptr_eq(&captured.borrow()[1], &rows[1]));
    rows[0].borrow_mut().uuid = "changed".into();
    assert_eq!(captured.borrow()[0].borrow().uuid, "changed");
    let types: SupportedTypes = Rc::new(RefCell::new(vec!["SPOT".into(), "INTX".into()]));
    let returned = types.clone();
    assert!(Rc::ptr_eq(&returned, &types));
    returned.borrow_mut().push("CFM".into());
    assert_eq!(&*types.borrow(), &["SPOT", "INTX", "CFM"]);
}

#[test]
fn balance_and_currency_aliases_keep_native_snapshots_and_nested_identity() {
    let raw = Rc::new(RefCell::new(Input::Array(vec![
        Input::Undefined,
        Input::Float(f64::NAN),
        Input::Text(vec![0xd800]),
    ])));
    let snapshot = Rc::new(CurrencyBalance::Declared(NativeBalance {
        free: f64::NAN,
        used: -0.0,
        total: f64::INFINITY,
        upnl: f64::NEG_INFINITY,
        info: Some(Input::Text(vec![0xd800])),
        currencies: None,
        coins: None,
    }));
    let balance: Balance = Rc::new(RefCell::new(PortfolioBalance {
        total: f64::INFINITY,
        free: -0.0,
        used: f64::NAN,
        upnl: f64::NEG_INFINITY,
        has_currencies: true,
        native_snapshot: Some(snapshot.clone()),
        extra: [("opaque".into(), raw.clone())].into_iter().collect(),
        ..Default::default()
    }));
    let child: Balance = Rc::new(RefCell::new(PortfolioBalance::default()));
    let prototype: Balance = Rc::new(RefCell::new(PortfolioBalance::default()));
    let currencies: Currencies = Rc::new(RefCell::new(PortfolioCurrencies::default()));
    currencies
        .borrow_mut()
        .own
        .insert("USDT".into(), child.clone());
    currencies.borrow_mut().prototype = Some(prototype.clone());
    balance.borrow_mut().currencies = Some(currencies.clone());
    let returned = balance.clone();
    assert!(Rc::ptr_eq(&returned, &balance));
    let observed = returned.borrow();
    assert_eq!(observed.total, f64::INFINITY);
    assert_eq!(observed.free.to_bits(), (-0.0_f64).to_bits());
    assert!(observed.used.is_nan());
    assert_eq!(observed.upnl, f64::NEG_INFINITY);
    assert!(observed.has_currencies);
    assert!(Rc::ptr_eq(
        observed.native_snapshot.as_ref().unwrap(),
        &snapshot
    ));
    assert!(Rc::ptr_eq(observed.extra.get("opaque").unwrap(), &raw));
    assert!(Rc::ptr_eq(
        observed.currencies.as_ref().unwrap(),
        &currencies
    ));
    assert!(Rc::ptr_eq(
        currencies.borrow().own.get("USDT").unwrap(),
        &child
    ));
    assert!(Rc::ptr_eq(
        currencies.borrow().prototype.as_ref().unwrap(),
        &prototype
    ));
    child.borrow_mut().free = -0.0;
    assert_eq!(
        currencies.borrow().own["USDT"].borrow().free.to_bits(),
        (-0.0_f64).to_bits()
    );
    let declared = snapshot.as_balance().unwrap();
    assert!(declared.free.is_nan());
    assert_eq!(declared.used.to_bits(), (-0.0_f64).to_bits());
    assert_eq!(declared.total, f64::INFINITY);
    assert_eq!(declared.upnl, f64::NEG_INFINITY);
    assert!(matches!(&declared.info, Some(Input::Text(units)) if units.as_slice() == [0xd800]));
    assert!(
        matches!(&*raw.borrow(), Input::Array(values) if values.len() == 3
        && matches!(values[0], Input::Undefined)
        && matches!(values[1], Input::Float(value) if value.is_nan())
        && matches!(&values[2], Input::Text(units) if units.as_slice() == [0xd800]))
    );
}

#[test]
fn failure_alias_preserves_the_native_error_carrier_and_message_units() {
    let original = Js::object(vec![
        ("name", Js::String(utf16("Error"))),
        ("message", Js::String(vec![0xd800].into())),
    ]);
    let first: PortfolioFailure = crate::binding::error::from_thrown(Thrown::new(original));
    let cloned: PortfolioFailure = first.clone();
    assert!(Rc::ptr_eq(&first, &cloned));
    let native = first.native().unwrap();
    assert!(
        native.origin.is_some(),
        "UTF16 callback cause must remain native"
    );
    let projected = crate::binding::error::exchange_error(&cloned);
    assert_eq!(projected.origin, native.origin);
    assert_eq!(projected, native.clone());
    let restored = Thrown::from(projected);
    assert!(
        matches!(restored.value.get("message").unwrap(), Js::String(units) if units.as_ref() == [0xd800])
    );
}
