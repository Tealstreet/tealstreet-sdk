//! Actual fixed alias decoders and their native container identities.
use safe_cex_venue_grvt::{
    builder_workflow::JsResult, composition::Fields, public_lifecycle::AddedMarkets,
};
use serde::Deserialize;
use serde_json::Value;
use std::sync::OnceLock;

#[test]
fn added_markets_bool_is_a_sequence_decoder() {
    let decoded: AddedMarkets<bool> = serde_json::from_str("[true,false]").unwrap();
    let rows = decoded.borrow();
    assert_eq!(rows.len(), 2);
    assert!(*rows[0].borrow());
    assert!(!*rows[1].borrow());
    assert!(!std::rc::Rc::ptr_eq(&rows[0], &rows[1]));
    drop(rows);
    for raw in ["null", "{}", "true", "[0]", "[null]"] {
        assert!(serde_json::from_str::<AddedMarkets<bool>>(raw).is_err());
    }
}

#[test]
fn js_result_bool_keeps_native_success_and_error_payloads() {
    let success: JsResult<bool, bool> = serde_json::from_str(r#"{"Ok":false}"#).unwrap();
    let failure: JsResult<bool, bool> = serde_json::from_str(r#"{"Err":true}"#).unwrap();
    assert_eq!(success, Ok(false));
    assert_eq!(failure, Err(true));
    for raw in [
        "null",
        "true",
        "[]",
        r#"{"Ok":0}"#,
        r#"{"Err":null}"#,
        r#"{"Other":true}"#,
    ] {
        assert!(serde_json::from_str::<JsResult<bool, bool>>(raw).is_err());
    }
}

#[test]
fn fields_text_keeps_declared_key_order_and_bool_values() {
    let raw: &'static str = r#"{"z":false,"a":true}"#;
    let fields: Fields<bool> = serde_json::from_str(raw).unwrap();
    let map = fields.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["z", "a"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [false, true]);
    assert_eq!(
        map.get_index(0).unwrap().0.as_ptr(),
        raw.as_ptr().wrapping_add(2)
    );
    assert_eq!(
        map.get_index(1).unwrap().0.as_ptr(),
        raw.as_ptr().wrapping_add(12)
    );
    drop(map);
    let other: Fields<bool> = serde_json::from_str(r#"{"second":true,"first":false}"#).unwrap();
    let map = other.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [true, false]);
}

#[test]
fn fields_borrowed_value_retains_decoded_escaped_keys() {
    // The fixture owns these strings for the program lifetime. The actual alias
    // retains &'static str keys; no key type or production lifetime is changed.
    static INPUT: OnceLock<Value> = OnceLock::new();
    let input: &'static Value =
        INPUT.get_or_init(|| serde_json::from_str(r#"{"\u007a":false,"a":true}"#).unwrap());
    let fields = <Fields<bool> as Deserialize>::deserialize(input).unwrap();
    let map = fields.borrow();
    assert_eq!(map.keys().copied().collect::<Vec<_>>(), ["z", "a"]);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [false, true]);
    let original_key = input.as_object().unwrap().get_key_value("z").unwrap().0;
    assert_eq!(map.get_index(0).unwrap().0.as_ptr(), original_key.as_ptr());
}

#[test]
fn fields_text_refuses_escaped_keys_and_owned_nonempty_values() {
    // Text decoding cannot borrow a key whose lexical escape requires a buffer.
    assert!(serde_json::from_str::<Fields<bool>>(r#"{"\u007a":false,"a":true}"#).is_err());
    // An owned Value cannot lend its member strings for the required lifetime.
    assert!(<Fields<bool> as Deserialize>::deserialize(serde_json::json!({"z":false})).is_err());
    let escaped: Value = serde_json::from_str(r#"{"\u007a":false,"a":true}"#).unwrap();
    assert!(<Fields<bool> as Deserialize>::deserialize(escaped).is_err());
    assert!(serde_json::from_str::<Fields<bool>>("{}").is_ok());
    let empty = <Fields<bool> as Deserialize>::deserialize(serde_json::json!({})).unwrap();
    assert!(empty.borrow().is_empty());
    static WRONG_BOOL: OnceLock<Value> = OnceLock::new();
    let wrong: &'static Value = WRONG_BOOL.get_or_init(|| serde_json::json!({"z":0}));
    assert!(<Fields<bool> as Deserialize>::deserialize(wrong).is_err());
    for raw in ["null", "[]", "true", r#"{"z":0}"#, r#"{"z":null}"#] {
        assert!(serde_json::from_str::<Fields<bool>>(raw).is_err());
    }
}

#[test]
fn fixed_callback_aliases_retain_native_values_and_errors() {
    use safe_cex_exchange::runtime::ExchangeError;
    use safe_cex_venue_grvt::{
        account_runtime, private_feeds, public_feeds, public_lifecycle, session_host,
    };
    use std::{cell::Cell, rc::Rc};

    let callback: session_host::Callback<bool> = Rc::new(|v| if v { Ok(false) } else { Err(true) });
    assert_eq!(callback(true), Ok(false));
    assert_eq!(callback(false), Err(true));
    let feed: public_feeds::FeedHandler<bool> = Rc::new(|v| {
        if v {
            Ok(())
        } else {
            Err(ExchangeError::new("feed error"))
        }
    });
    assert!(feed(true).is_ok());
    assert_eq!(feed(false).unwrap_err().message, "feed error");
    let cleanup: public_feeds::FeedCleanup = Rc::new(|| Err(ExchangeError::new("cleanup error")));
    assert_eq!(cleanup().unwrap_err().message, "cleanup error");
    let projector: public_feeds::LevelProjector<bool> = Rc::new(|a, b, c| Ok((a && b) || c));
    assert!(!projector(true, false, false).unwrap());
    assert!(projector(false, false, true).unwrap());
    let mut timer: public_lifecycle::TimerCallback =
        Box::new(|| Err(ExchangeError::new("timer error")));
    assert_eq!(timer().unwrap_err().message, "timer error");
    let count = Rc::new(Cell::new(0));
    let observed = count.clone();
    let timer: private_feeds::TimerCallback = Rc::new(move || observed.set(observed.get() + 1));
    timer();
    timer();
    assert_eq!(count.get(), 2);

    let mut predicate = |v| if v { Ok(false) } else { Err(true) };
    let predicate: &mut private_feeds::Predicate<'static, bool> = &mut predicate;
    assert_eq!(predicate(true), Ok(false));
    assert_eq!(predicate(false), Err(true));
    let mut visitor = |v| if v { Ok(()) } else { Err(false) };
    let visitor: &mut private_feeds::Visitor<'static, bool> = &mut visitor;
    assert_eq!(visitor(true), Ok(()));
    assert_eq!(visitor(false), Err(false));
    let mut iteration = |v| if v { Ok(true) } else { Err(false) };
    let iteration: &mut account_runtime::Iteration<'static, bool> = &mut iteration;
    assert_eq!(iteration(true), Ok(true));
    assert_eq!(iteration(false), Err(false));
}

#[test]
fn promise_and_reaction_aliases_execute_actual_future_outputs() {
    use safe_cex_venue_grvt::builder_workflow::{JsPromise, Reaction};
    use std::{
        cell::Cell,
        future::Future,
        rc::Rc,
        task::{Context, Poll, Waker},
    };
    let mut cx = Context::from_waker(Waker::noop());
    let mut success: JsPromise<bool> = Box::pin(async { Ok(false) });
    let mut error: JsPromise<bool> = Box::pin(async { Err(true) });
    assert_eq!(
        Future::poll(success.as_mut(), &mut cx),
        Poll::Ready(Ok(false))
    );
    assert_eq!(
        Future::poll(error.as_mut(), &mut cx),
        Poll::Ready(Err(true))
    );
    let flag = Rc::new(Cell::new(false));
    let observed = flag.clone();
    let mut reaction: Reaction = Box::pin(async move {
        observed.set(true);
    });
    assert_eq!(Future::poll(reaction.as_mut(), &mut cx), Poll::Ready(()));
    assert!(flag.get());
}

#[test]
fn handler_array_alias_preserves_holes_undefined_and_shared_callbacks() {
    use safe_cex_venue_grvt::public_feeds::{HandlerArray, HandlerSlot};
    use std::{
        cell::{Cell, RefCell},
        rc::Rc,
    };
    let flag = Rc::new(Cell::new(false));
    let observed = flag.clone();
    let handlers: HandlerArray<bool> = Rc::new(RefCell::new(vec![
        HandlerSlot::Hole,
        HandlerSlot::Undefined,
        HandlerSlot::Callback(Rc::new(move |v| {
            observed.set(v);
            Ok(())
        })),
    ]));
    let cloned = handlers.clone();
    assert!(Rc::ptr_eq(&handlers, &cloned));
    let rows = cloned.borrow();
    assert_eq!(rows.len(), 3);
    assert!(matches!(rows[0], HandlerSlot::Hole));
    assert!(matches!(rows[1], HandlerSlot::Undefined));
    let HandlerSlot::Callback(callback) = &rows[2] else {
        panic!("native handler slot identity")
    };
    callback(true).unwrap();
    assert!(flag.get());
    callback(false).unwrap();
    assert!(!flag.get());
}

#[test]
fn stream_feed_alias_is_an_ordered_string_sequence_decoder() {
    use safe_cex_venue_grvt::private_commands::StreamFeed;
    let feed: StreamFeed = serde_json::from_str(r#"["second","first"]"#).unwrap();
    assert_eq!(*feed.borrow(), ["second", "first"]);
    let empty: StreamFeed = serde_json::from_str("[]").unwrap();
    assert!(empty.borrow().is_empty());
    for raw in ["null", "{}", "true", r#"["ok",1]"#] {
        assert!(serde_json::from_str::<StreamFeed>(raw).is_err());
    }
}

#[test]
fn nonce_aliases_keep_key_order_presence_text_and_number_bits() {
    use safe_cex_venue_grvt::composition::{NonceIds, NonceTimes};
    let ids: NonceIds = serde_json::from_str(r#"{"z":"0x02","a":"0x01"}"#).unwrap();
    let ids = ids.borrow();
    assert_eq!(
        ids.keys().map(String::as_str).collect::<Vec<_>>(),
        ["z", "a"]
    );
    assert_eq!(
        ids.values().map(String::as_str).collect::<Vec<_>>(),
        ["0x02", "0x01"]
    );
    let times: NonceTimes =
        serde_json::from_str(r#"{"second":null,"first":-0.0,"third":2.5}"#).unwrap();
    let times = times.borrow();
    assert_eq!(
        times.keys().map(String::as_str).collect::<Vec<_>>(),
        ["second", "first", "third"]
    );
    assert_eq!(times.get("second"), Some(&None));
    assert_eq!(times.get("absent"), None);
    assert_eq!(times["first"].unwrap().to_bits(), (-0.0_f64).to_bits());
    assert_eq!(times["third"].unwrap().to_bits(), 2.5_f64.to_bits());
    for raw in ["null", "[]", r#"{"id":false}"#] {
        assert!(serde_json::from_str::<NonceIds>(raw).is_err());
        assert!(serde_json::from_str::<NonceTimes>(raw).is_err());
    }
}

#[test]
fn market_ref_alias_decodes_native_fields_flags_and_nested_number_bits() {
    use safe_cex_venue_grvt::public_feeds::MarketRef;
    let raw = r#"{"id":"native-id","symbol":"NATIVE","base":"B","quote":"Q","active":false,"precision":{"amount":0.125,"price":0.25},"limits":{"amount":{"min":0.5,"max":10},"leverage":{"min":1,"max":20}},"isPerpetual":false,"info":null,"takerFee":0.1,"makerFee":0.2}"#;
    let market: MarketRef = serde_json::from_str(raw).unwrap();
    let market = market.borrow();
    assert_eq!(market.id, "native-id");
    assert_eq!(market.symbol, "NATIVE");
    assert!(!market.active);
    assert_eq!(market.is_perpetual, Some(false));
    assert!(market.is_spot.is_none());
    assert_eq!(market.precision.amount.to_bits(), 0.125_f64.to_bits());
    assert_eq!(market.precision.price.to_bits(), 0.25_f64.to_bits());
    assert_eq!(market.limits.leverage.max.to_bits(), 20.0_f64.to_bits());
    assert!(market.info.as_ref().unwrap().is_null());
    for raw in ["null", "[]", "true", "{}"] {
        assert!(serde_json::from_str::<MarketRef>(raw).is_err());
    }
}
