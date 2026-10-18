//! Receiver witnesses use the assembled production exchange, never a fabricated Scope.
#[path = "binding_runtime.rs"]
mod terminal;

use super::{BitrueDependencies, BitrueExchange};
use crate::{
    collection_reads::ReadHost,
    composition::CompositionHost,
    private_auth::PrivateHost,
    public_bodies::PublicBodyHost,
    public_streams::StreamHost,
    runtime_lifecycle::LifecycleHost,
    trading_rest::TradingHost,
    ws_sync::{ParseHost, SubscriptionHost},
};
use safe_cex_exchange::{
    exchange::Exchange,
    js_runtime::{
        client::JsHttpHost,
        promise::JsFuture,
        value::{Js, Thrown},
    },
    runtime::ExchangeRuntime,
};
use std::rc::Rc;

struct NoHttp(Rc<terminal::Runtime>);
impl JsHttpHost for NoHttp {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.0.clone()
    }
    fn send(&self, _: Js) -> JsFuture<'static> {
        panic!("receiver witness unexpectedly reached terminal HTTP")
    }
}
struct Fixture {
    venue: Rc<BitrueExchange>,
    runtime: Rc<terminal::Runtime>,
}
impl Fixture {
    fn new() -> Self {
        let runtime = Rc::new(terminal::Runtime::default());
        let venue = BitrueExchange::new(
            safe_cex_types::ExchangeName::Bitrue,
            serde_json::from_value(
                serde_json::json!({"key":"fixture-key","secret":"fixture-secret"}),
            )
            .unwrap(),
            Rc::new(safe_cex_exchange::native_store::DefaultStore::new()),
            BitrueDependencies {
                runtime: runtime.clone(),
                http: Rc::new(NoHttp(runtime.clone())),
                external_public_data: None,
            },
        )
        .unwrap();
        runtime.advance(0.0);
        Self { venue, runtime }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.venue.dispose().unwrap();
    }
}
fn text(value: Js) -> String {
    String::from_utf16(&value.to_js_string().unwrap()).unwrap()
}
fn bits(value: Js) -> u64 {
    let Js::Number(number) = value else {
        panic!("expected native Number")
    };
    number.to_bits()
}

#[test]
fn collection_receiver_preserves_handles_keys_and_native_bits() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let marker = Js::object(vec![]);
    let object = ReadHost::object(
        h,
        vec![
            ("ownUndefined", Js::Undefined),
            ("negativeZero", Js::Number(-0.0)),
            ("marker", marker.clone()),
        ],
    );
    assert!(object.has_own("ownUndefined"));
    assert!(
        ReadHost::get(h, &object, "marker")
            .unwrap()
            .same_reference(&marker),
        "ReadHost::get retains native identity"
    );
    assert_eq!(
        bits(ReadHost::get(h, &object, "negativeZero").unwrap()),
        (-0.0f64).to_bits()
    );
    ReadHost::set(h, &object, "updated", marker.clone()).unwrap();
    assert!(
        object.get("updated").unwrap().same_reference(&marker),
        "ReadHost::set keeps the supplied handle"
    );
    assert_eq!(text(ReadHost::text(h, "a😀b")), "a😀b");
    assert_eq!(
        text(ReadHost::split_at(h, Js::str("a::b::c"), "::", 1).unwrap()),
        "b",
        "split_at selects the requested segment"
    );
    assert!(
        ReadHost::split_at(h, Js::str("a"), "::", 3)
            .unwrap()
            .is_undefined()
    );
    assert_eq!(
        text(ReadHost::table(h, "ORDER_SIDE", Js::str("SELL")).unwrap()),
        "sell",
        "ReadHost::table uses the actual installed table"
    );
    ReadHost::console(h, "error", "receiver-observation").unwrap();
    assert!(
        f.runtime
            .console
            .borrow()
            .iter()
            .any(|s| s == "receiver-observation"),
        "console reaches the original runtime"
    );
}

#[test]
fn socket_receivers_keep_lookup_identity_and_callback_order() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let sell = ParseHost::lookup(h, "ORDER_SIDE", "SELL".encode_utf16().collect()).unwrap();
    assert_eq!(
        text(sell),
        "sell",
        "ParseHost::lookup reaches the installed source enum"
    );
    assert!(
        ParseHost::lookup(h, "ORDER_SIDE", "missing".encode_utf16().collect())
            .unwrap()
            .is_undefined()
    );
    let inherited =
        ParseHost::lookup(h, "ORDER_SIDE", "toString".encode_utf16().collect()).unwrap();
    assert!(inherited.same_reference(&h.global("ORDER_SIDE").unwrap().get("toString").unwrap()));
    assert_eq!(text(PublicBodyHost::text(h, "socket😀")), "socket😀");
    let marker = Js::object(vec![]);
    assert!(
        PublicBodyHost::object(h, vec![("marker", marker.clone())])
            .get("marker")
            .unwrap()
            .same_reference(&marker)
    );
    assert!(
        PublicBodyHost::global(h, "ORDER_SIDE")
            .unwrap()
            .same_reference(&h.global("ORDER_SIDE").unwrap()),
        "PublicBodyHost::global retains the actual global"
    );
    let public = h.this.get("publicWebsocket").unwrap();
    let socket_host = h.with_this(public.clone());
    let bound = PublicBodyHost::global(&socket_host, "WebSocket").unwrap();
    assert!(
        !bound
            .call(
                Js::Undefined,
                vec![Js::str("wss://example.invalid/receiver")]
            )
            .unwrap()
            .is_undefined()
    );
    assert!(
        StreamHost::global(h, "ORDER_SIDE")
            .unwrap()
            .same_reference(&h.global("ORDER_SIDE").unwrap()),
        "StreamHost::global keeps the installed handle"
    );
    let mut seen = vec![];
    assert!(
        SubscriptionHost::channels_for_each(h, &["first", "second", "third"], |v| {
            seen.push(text(v));
            Ok(())
        })
        .unwrap()
        .is_undefined()
    );
    assert_eq!(
        seen,
        ["first", "second", "third"],
        "SubscriptionHost preserves channel order"
    );
    let error = Thrown::new(marker.clone());
    let mut seen = vec![];
    let rejected = SubscriptionHost::channels_for_each(h, &["first", "stop", "never"], |v| {
        let s = text(v);
        seen.push(s.clone());
        if s == "stop" {
            Err(error.clone())
        } else {
            Ok(())
        }
    })
    .unwrap_err();
    assert_eq!(seen, ["first", "stop"]);
    assert!(
        rejected.value.same_reference(&marker),
        "callback rejection retains original identity"
    );
}

#[test]
fn lifecycle_and_trading_receivers_keep_presence_and_delete_semantics() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    assert!(
        LifecycleHost::builtin(h, "setTimeout")
            .unwrap()
            .same_reference(&h.global("setTimeout").unwrap()),
        "LifecycleHost::builtin selects the installed capability"
    );
    let parent = Js::object(vec![("inherited", Js::Undefined)]);
    let object = Js::object(vec![]);
    h.global("Object")
        .unwrap()
        .call_method("setPrototypeOf", vec![object.clone(), parent])
        .unwrap();
    object.define("own", Js::Undefined).unwrap();
    assert!(TradingHost::has(h, object.clone(), "inherited").unwrap());
    assert!(
        TradingHost::has(h, object.clone(), "own").unwrap(),
        "has distinguishes own undefined from absence"
    );
    assert!(!TradingHost::has(h, object.clone(), "absent").unwrap());
    assert!(TradingHost::has(h, Js::Number(1.0), "own").is_err());
    TradingHost::remove_key(h, object.clone(), "own").unwrap();
    assert!(
        !object.has_own("own"),
        "TradingHost::remove_key deletes rather than writing undefined"
    );
    assert!(TradingHost::has(h, object.clone(), "inherited").unwrap());
    object.define("locked", Js::Number(3.0)).unwrap();
    h.global("Object")
        .unwrap()
        .call_method("freeze", vec![object.clone()])
        .unwrap();
    assert!(
        TradingHost::remove_key(h, object.clone(), "locked").is_err(),
        "nonconfigurable deletion refusal propagates"
    );
    assert!(TradingHost::remove_key(h, object, "inherited").is_ok());
}

#[test]
fn private_receiver_forwards_native_strings_hmac_and_members() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    assert!(
        PrivateHost::contains(h, Js::str("alpha😀omega"), "😀").unwrap(),
        "PrivateHost::contains uses original string receiver"
    );
    assert!(!PrivateHost::contains(h, Js::str("alpha"), "omega").unwrap());
    assert!(PrivateHost::starts_with(h, Js::str("😀alpha"), "😀").unwrap());
    assert!(!PrivateHost::starts_with(h, Js::str("alpha😀"), "😀").unwrap());
    let split = PrivateHost::split(h, Js::str("first:second:"), ":")
        .unwrap()
        .array_values()
        .unwrap();
    assert_eq!(
        split.into_iter().map(text).collect::<Vec<_>>(),
        ["first", "second", ""]
    );
    let mac = PrivateHost::create_hmac(h, "sha256", Js::str("key")).unwrap();
    let digest = mac
        .call_method(
            "update",
            vec![Js::str("The quick brown fox jumps over the lazy dog")],
        )
        .unwrap()
        .call_method("digest", vec![Js::str("hex")])
        .unwrap();
    assert_eq!(
        text(digest),
        "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8",
        "PrivateHost HMAC uses the real shared crypto"
    );
    let client = h.this.get("xhr").unwrap();
    let options = h.this.get("options").unwrap();
    PrivateHost::transport(
        h,
        client.clone(),
        options.clone(),
        Js::str("bitrue"),
        "bigInt",
    )
    .unwrap();
    assert!(
        PrivateHost::transport(
            h,
            client.clone(),
            options.clone(),
            Js::str("invalid-venue"),
            "bigInt"
        )
        .is_err()
    );
    assert!(PrivateHost::transport(h, client, options, Js::str("bitrue"), "unsupported").is_err());
    let marker = Js::object(vec![]);
    let object = Js::object(vec![]);
    PrivateHost::define(h, object.clone(), "marker", marker.clone()).unwrap();
    assert!(object.get("marker").unwrap().same_reference(&marker));
    let private = h.this.get("privateWebsocket").unwrap();
    let member = PrivateHost::member(h, "isSuccessfulCode", "source member", private);
    assert!(
        matches!(
            member.call(Js::Undefined, vec![Js::Number(200.0)]).unwrap(),
            Js::Bool(true)
        ),
        "PrivateHost::member calls real source body"
    );
    assert!(matches!(
        member.call(Js::Undefined, vec![Js::Number(201.0)]).unwrap(),
        Js::Bool(false)
    ));
    let returned = marker.clone();
    let callback = PrivateHost::named_function(
        h,
        "privateReceiver",
        Rc::new(move |args| {
            assert_eq!(bits(args[0].clone()), (-0.0f64).to_bits());
            Ok(returned.clone())
        }),
    );
    assert_eq!(text(callback.get("name").unwrap()), "privateReceiver");
    assert!(
        callback
            .call(Js::Undefined, vec![Js::Number(-0.0)])
            .unwrap()
            .same_reference(&marker)
    );
    PrivateHost::class_environment(h, &crate::private_auth::RETRY_DELAYS, "dispose");
    assert_eq!(
        h.global("listenKeyRetryDelays")
            .unwrap()
            .array_values()
            .unwrap()
            .into_iter()
            .map(bits)
            .collect::<Vec<_>>(),
        crate::private_auth::RETRY_DELAYS
            .iter()
            .map(|n| n.to_bits())
            .collect::<Vec<_>>()
    );
}

#[test]
fn composition_receiver_uses_live_factories_and_lexical_members() {
    let f = Fixture::new();
    let h = &f.venue.rt;
    let holder = Js::object(vec![]);
    let handle = Js::object(vec![]);
    CompositionHost::define(h, holder.clone(), "marker", handle.clone()).unwrap();
    assert!(holder.get("marker").unwrap().same_reference(&handle));
    let rule = CompositionHost::rule(h, "fixture rule", "^a+$").unwrap();
    assert_eq!(text(rule.get("label").unwrap()), "fixture rule");
    assert!(
        matches!(
            rule.call_method("validate", vec![Js::str("aaa")]).unwrap(),
            Js::Bool(true)
        ),
        "CompositionHost::rule uses actual shared regex"
    );
    assert!(matches!(
        rule.call_method("validate", vec![Js::str("b")]).unwrap(),
        Js::Bool(false)
    ));
    let every = CompositionHost::grafana(h, "every")
        .unwrap()
        .array_values()
        .unwrap();
    let periodic = CompositionHost::grafana(h, "periodic")
        .unwrap()
        .array_values()
        .unwrap();
    assert_eq!(
        every.into_iter().map(text).collect::<Vec<_>>(),
        ["/fapi/v2/order"]
    );
    assert_eq!(
        periodic.into_iter().map(text).collect::<Vec<_>>(),
        ["/fapi/v2/openOrders", "/fapi/v2/account"]
    );
    assert!(
        CompositionHost::binding(h, "createAPI")
            .unwrap()
            .same_reference(&h.global("createAPI").unwrap()),
        "CompositionHost::binding retains actual factory identity"
    );
    for kind in ["public", "private"] {
        let socket = CompositionHost::construct_socket(h, kind, h.this.clone()).unwrap();
        assert!(
            socket.get("parent").unwrap().same_reference(&h.this),
            "constructed sockets retain the actual parent"
        );
        assert!(socket.get("onMessage").unwrap().is_callable());
        socket.call_method("dispose", vec![]).unwrap();
    }
    let member =
        CompositionHost::member(h, "normalizeTimestampMs", "source member", h.this.clone());
    assert_eq!(
        bits(
            member
                .call(Js::Undefined, vec![Js::Number(1_700_000_000_000.0)])
                .unwrap()
        ),
        1_700_000_000_000.0f64.to_bits()
    );
    let marker = Js::object(vec![]);
    let expected = marker.clone();
    let callback = CompositionHost::named_function(
        h,
        "compositionReceiver",
        Rc::new(move |args| {
            assert!(args[0].same_reference(&expected));
            Ok(Js::Number(-0.0))
        }),
    );
    assert_eq!(text(callback.get("name").unwrap()), "compositionReceiver");
    assert_eq!(
        bits(callback.call(Js::Undefined, vec![marker]).unwrap()),
        (-0.0f64).to_bits()
    );
}
