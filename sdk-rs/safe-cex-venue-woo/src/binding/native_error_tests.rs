use super::*;
use crate::binding_test_harness::Runtime;
use safe_cex_exchange::{
    js_runtime::{
        client::{JsHttpHost, JsRestAdapter},
        promise::JsFuture,
        value::{Js, Thrown, error_object},
    },
    native_store::DefaultStore,
    runtime::{ExchangeError, ExchangeRuntime},
};
use safe_cex_transport::{RestAdapter, RestRequest};
use std::{
    cell::RefCell,
    future::Future,
    rc::Rc,
    task::{Context, Poll, Waker},
};

struct Host {
    runtime: Rc<Runtime>,
    original: Js,
    requests: Rc<RefCell<Vec<Js>>>,
}
impl JsHttpHost for Host {
    fn runtime(&self) -> Rc<dyn ExchangeRuntime> {
        self.runtime.clone()
    }
    fn send(&self, config: Js) -> JsFuture<'static> {
        if self.original.is_object() {
            config
                .get("paramsSerializer")
                .unwrap()
                .set("indexes", Js::Bool(false))
                .unwrap();
            self.original.define("config", config.clone()).unwrap();
        }
        self.requests.borrow_mut().push(config);
        let original = self.original.clone();
        Box::pin(async move { Err(Thrown::new(original)) })
    }
}
fn drive<T>(runtime: &Runtime, future: impl Future<Output = T>) -> T {
    let mut future = std::pin::pin!(future);
    for _ in 0..100 {
        runtime.pump();
        if let Poll::Ready(value) = future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            return value;
        }
        runtime.advance(100.0);
    }
    panic!("native WOO error must settle")
}
#[test]
fn constructed_woo_client_keeps_error_fields_through_typed_rest_adapter() {
    let runtime = Rc::new(Runtime::default());
    runtime.now.set(1_700_000_000_000.0);
    let original = error_object("TypeError", "native WOO rejection");
    for (field, value) in [
        ("code", Js::str("ERR_WOO_REJECTED")),
        (
            "stack",
            Js::str("TypeError: native WOO rejection\n  at terminal-host"),
        ),
        ("isAxiosError", Js::Bool(true)),
        (
            "cause",
            Js::object(vec![("message", Js::str("native root cause"))]),
        ),
    ] {
        original.define(field, value).unwrap();
    }
    original
        .define(
            "response",
            Js::object(vec![
                ("status", Js::Number(418.0)),
                ("headers", Js::object(vec![])),
                ("data", Js::object(vec![("message", Js::str("denied"))])),
            ]),
        )
        .unwrap();
    original
        .define(
            "extra",
            Js::object(vec![
                ("number", Js::Number(f64::INFINITY)),
                ("missing", Js::Undefined),
                ("text", Js::utf16(&[0xd800])),
            ]),
        )
        .unwrap();
    let requests = Rc::new(RefCell::new(vec![]));
    let exchange = WooExchange::new(
        safe_cex_types::ExchangeName::Woo,
        crate::binding_test_harness::options(),
        Rc::new(DefaultStore::new()),
        WooDependencies {
            runtime: runtime.clone(),
            http: Rc::new(Host {
                runtime: runtime.clone(),
                original: original.clone(),
                requests: requests.clone(),
            }),
            external_public_data: None,
        },
    )
    .unwrap();
    let adapter = JsRestAdapter::new(
        exchange.rt.exchange_runtime(),
        exchange.rt.this.get("xhr").unwrap(),
    );
    let failure = drive(
        &runtime,
        adapter.execute(RestRequest {
            method: "GET".into(),
            url: "/v3/accountinfo".into(),
            ..Default::default()
        }),
    )
    .unwrap_err();
    let native = ExchangeError::from(failure);
    let error = Thrown::from(native);
    assert_eq!(
        error.value.get("message").unwrap().to_display(),
        "[woo] GET /v3/accountinfo failed with 418 undefined: denied"
    );
    let context = error.value.get("context").unwrap();
    assert_eq!(context.get("method").unwrap().to_display(), "GET");
    assert_eq!(context.get("url").unwrap().to_display(), "/v3/accountinfo");
    assert_eq!(
        context.get("statusCode").unwrap().to_number().unwrap(),
        418.0
    );
    assert_eq!(
        error.value.has_own("name"),
        original.has_own("name"),
        "native TypeError name remains inherited"
    );
    for field in ["name", "message", "code", "stack", "isAxiosError"] {
        assert!(
            error
                .value
                .get(field)
                .unwrap()
                .strict_equals(&original.get(field).unwrap()),
            "WOO-NATIVE-ERROR must preserve {field}"
        );
    }
    assert_eq!(
        error
            .value
            .get("cause")
            .unwrap()
            .get("message")
            .unwrap()
            .to_display(),
        "native root cause"
    );
    assert_eq!(
        error
            .value
            .get("response")
            .unwrap()
            .get("status")
            .unwrap()
            .to_number()
            .unwrap(),
        418.0
    );
    assert_eq!(
        error
            .value
            .get("response")
            .unwrap()
            .get("data")
            .unwrap()
            .get("message")
            .unwrap()
            .to_display(),
        "denied"
    );
    let Js::Object(object) = &error.value else {
        panic!("native Error instance")
    };
    assert_eq!(object.class, "Error");
    assert!(
        !error
            .value
            .get("originalError")
            .unwrap()
            .strict_equals(&error.value),
        "approved originalError snapshot must not point back to the Error"
    );
    let snapshot = error.value.get("originalError").unwrap();
    assert!(matches!(&snapshot, Js::Object(object) if object.class == "Error"));
    assert!(!snapshot.has_own("originalError"));
    assert_eq!(
        snapshot
            .get("config")
            .unwrap()
            .get("url")
            .unwrap()
            .to_display(),
        "/v3/accountinfo"
    );
    assert_eq!(
        snapshot
            .get("config")
            .unwrap()
            .get("method")
            .unwrap()
            .to_display(),
        "GET"
    );
    let serializer = snapshot
        .get("config")
        .unwrap()
        .get("paramsSerializer")
        .unwrap();
    assert!(
        serializer
            .get("indexes")
            .unwrap()
            .strict_equals(&Js::Bool(false))
    );
    assert!(
        !serializer.has_own("serialize"),
        "executable config callback is not owned data"
    );
    for field in ["config", "extra"] {
        assert!(
            snapshot
                .get(field)
                .unwrap()
                .strict_equals(&error.value.get(field).unwrap()),
            "owned original Error snapshot shares its captured {field} data"
        );
    }
    let extra = snapshot.get("extra").unwrap();
    assert_eq!(
        extra.get("number").unwrap().to_number().unwrap(),
        f64::INFINITY
    );
    assert!(extra.has_own("missing") && extra.get("missing").unwrap().is_undefined());
    assert_eq!(
        extra.get("text").unwrap().to_js_string().unwrap().as_ref(),
        &[0xd800]
    );

    for field in [
        "name",
        "message",
        "code",
        "stack",
        "isAxiosError",
        "enhancedMessage",
    ] {
        assert!(
            snapshot
                .get(field)
                .unwrap()
                .strict_equals(&error.value.get(field).unwrap()),
            "original Error snapshot preserves {field}"
        );
    }

    assert_eq!(requests.borrow().len(), 1);
    assert!(
        !requests.borrow()[0]
            .get("headers")
            .unwrap()
            .get("x-api-signature")
            .unwrap()
            .to_display()
            .is_empty(),
        "production WOO signer executed before terminal rejection"
    );
    use safe_cex_exchange::exchange::Exchange;
    exchange.dispose().unwrap();
    let Js::Object(value) = &error.value else {
        panic!("materialized Error")
    };
    let weak = Rc::downgrade(value);
    drop(error);
    assert!(
        weak.upgrade().is_none(),
        "ERROR-SNAPSHOT-RELEASE must not retain an Rc self cycle"
    );
}

#[test]
fn raw_client_primitive_rejections_keep_typed_diagnostics() {
    for original in [
        Js::str("primitive rejection"),
        Js::Number(42.0),
        Js::Null,
        Js::Undefined,
    ] {
        let runtime = Rc::new(Runtime::default());
        let host = Rc::new(Host {
            runtime: runtime.clone(),
            original: original.clone(),
            requests: Rc::new(RefCell::new(vec![])),
        });
        let client = safe_cex_exchange::js_runtime::client::create(host, Js::object(vec![]));
        let adapter = JsRestAdapter::new(runtime.clone(), client);
        let failure = drive(
            &runtime,
            adapter.execute(RestRequest {
                method: "GET".into(),
                url: "/primitive".into(),
                ..Default::default()
            }),
        )
        .unwrap_err();
        assert_eq!(
            ExchangeError::from(failure).message,
            original.to_display(),
            "RAW-REST primitive diagnostic survives typed projection"
        );
    }
}

#[test]
fn constructed_woo_factory_enhances_primitive_rejections_through_typed_bridge() {
    for original in [
        Js::str("bare rejection"),
        Js::Number(42.0),
        Js::Bool(false),
        Js::utf16(&[0xd800]),
    ] {
        let runtime = Rc::new(Runtime::default());
        runtime.now.set(1_800_000_000_000.0);
        let exchange = WooExchange::new(
            safe_cex_types::ExchangeName::Woo,
            crate::binding_test_harness::options(),
            Rc::new(DefaultStore::new()),
            WooDependencies {
                runtime: runtime.clone(),
                http: Rc::new(Host {
                    runtime: runtime.clone(),
                    original: original.clone(),
                    requests: Rc::default(),
                }),
                external_public_data: None,
            },
        )
        .unwrap();
        let adapter = JsRestAdapter::new(
            exchange.rt.exchange_runtime(),
            exchange.rt.this.get("xhr").unwrap(),
        );
        let failure = drive(
            &runtime,
            adapter.execute(RestRequest {
                method: "GET".into(),
                url: "/v3/accountinfo".into(),
                ..Default::default()
            }),
        )
        .unwrap_err();
        let error = Thrown::from(ExchangeError::from(failure));
        assert_eq!(
            error.value.get("message").unwrap().to_display(),
            "[woo] Request setup error: undefined",
            "WOO-FACTORY-ENHANCER runs before downstream handling"
        );
        assert_eq!(error.value.get("name").unwrap().to_display(), "Error");
        assert!(matches!(&error.value, Js::Object(object) if object.class == "Error"));
        assert!(error.value.get("isAxiosError").unwrap().truthy());
        assert!(
            error
                .value
                .get("originalError")
                .unwrap()
                .strict_equals(&original)
        );
        assert!(
            error
                .value
                .get("enhancedMessage")
                .unwrap()
                .strict_equals(&error.value.get("message").unwrap())
        );
        let timestamp = error
            .value
            .get("context")
            .unwrap()
            .get("timestamp")
            .unwrap();
        let parsed =
            safe_cex_num::js_date_parse_utf16(&timestamp.to_js_string().unwrap(), |time| time);
        assert!(parsed >= 1_800_000_000_000.0 && parsed <= runtime.now.get());
        assert_eq!(
            safe_cex_num::js_date_to_iso_string(parsed).unwrap(),
            timestamp.to_display()
        );
        use safe_cex_exchange::exchange::Exchange;
        exchange.dispose().unwrap();
    }
}
