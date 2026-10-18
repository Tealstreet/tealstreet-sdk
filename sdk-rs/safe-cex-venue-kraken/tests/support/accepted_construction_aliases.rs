//! Constructor callbacks at the actual constructed native Scope and signed clients.
use super::{
    Rt,
    accepted_read_receivers::terminal::{Harness, harness, options},
};
use crate::{
    construction::{BaseUrl, ConstructorHost, Factory, RateLimit, Venue},
    rest_account::Handle,
};
use safe_cex_exchange::js_runtime::{
    promise::await_js,
    value::{Js, Thrown},
};
use safe_cex_types::ExchangeName;
use std::{
    rc::Rc,
    task::{Context, Poll, Waker},
};

fn settle(h: &Harness, promise: Js) -> Js {
    h.drive(Box::pin(async {
        await_js(&h.exchange.rt.exchange_runtime(), promise)
            .await
            .map_err(|e| e.to_exchange_error())
    }))
    .unwrap()
}

#[test]
fn factory_alias_selects_source_bases_and_real_signed_client_pipeline() {
    for (name, venue) in [
        (ExchangeName::Kraken, Venue::Futures),
        (ExchangeName::Krakenspot, Venue::Spot),
    ] {
        let h = harness(name.clone(), options(), |http| {
            http.ok(
                "GET",
                "/v3/accounts",
                serde_json::json!({"result":"success"}),
            );
            http.ok(
                "POST",
                "/0/private/Balance",
                serde_json::json!({"error":[],"result":{}}),
            );
        });
        let rt = &h.exchange.rt;
        let factory: Factory<'_, Rt> = Rc::new(move |options, receiver, base| {
            ConstructorHost::create_api(rt, venue, options, receiver, base)
        });
        let independently_typed: &Rc<
            dyn Fn(Rc<Js>, Rc<Js>, BaseUrl) -> Result<Rc<Js>, Thrown> + '_,
        > = &factory;
        for base in [BaseUrl::Derivatives, BaseUrl::History] {
            let client = independently_typed(
                Rc::new(rt.this.get("options").unwrap()),
                Rc::new(rt.this.clone()),
                base,
            )
            .unwrap();
            assert!(client.get("get").unwrap().is_callable());
            assert!(client.get("post").unwrap().is_callable());
            let promise = if venue == Venue::Futures {
                client
                    .call_method("get", vec![Js::str("/v3/accounts")])
                    .unwrap()
            } else {
                client
                    .call_method(
                        "post",
                        vec![Js::str("/0/private/Balance"), Js::object(vec![])],
                    )
                    .unwrap()
            };
            settle(&h, promise);
            let calls = h.http.requests.borrow();
            let wire = calls.last().unwrap();
            let expected = match (venue, base) {
                (Venue::Futures, BaseUrl::Derivatives) => {
                    "https://futures.kraken.com/derivatives/api"
                }
                (Venue::Futures, BaseUrl::History) => "https://futures.kraken.com/api",
                (Venue::Spot, _) => "https://api.kraken.com",
            };
            assert_eq!(wire.base_url, expected);
            let (key, signature) = if venue == Venue::Futures {
                ("APIKey", "Authent")
            } else {
                ("API-Key", "API-Sign")
            };
            assert_eq!(wire.headers[key], "fixture-key");
            assert!(!wire.headers[signature].as_str().unwrap().is_empty());
        }
        // Exercise the production-created callbacks as well as the typed alias
        // front door: construct_with feeds Factory into secondary_client.
        settle(
            &h,
            rt.this
                .get("xhr")
                .unwrap()
                .call_method("get", vec![Js::str("/v3/accounts")])
                .unwrap(),
        );
        let calls = h.http.requests.borrow();
        assert_eq!(calls.len(), 3);
        assert_eq!(
            calls.last().unwrap().base_url,
            if venue == Venue::Futures {
                "https://futures.kraken.com/derivatives/api"
            } else {
                "https://api.kraken.com"
            }
        );
        assert!(
            rt.this
                .get("xhr")
                .unwrap()
                .strict_equals(&rt.this.get("_primaryXhr").unwrap())
        );
        assert!(
            rt.this
                .get("xhr")
                .unwrap()
                .strict_equals(&rt.this.get("_secondaryXhr").unwrap())
        );
    }
}

#[test]
fn factory_alias_preserves_native_option_getter_failure_before_any_http() {
    let h = harness(ExchangeName::Kraken, options(), |_| {});
    let rt = &h.exchange.rt;
    let factory: Factory<'_, Rt> = Rc::new(move |options, receiver, base| {
        ConstructorHost::create_api(rt, Venue::Futures, options, receiver, base)
    });
    let original = Js::object(vec![
        ("units", Js::utf16(&[0xd800])),
        ("number", Js::Number(-0.0)),
    ]);
    let thrown = original.clone();
    let opts = Js::object(vec![]);
    opts.define_accessor(
        "testnet",
        Some(Rc::new(move |_| Err(Thrown::new(thrown.clone())))),
        None,
    )
    .unwrap();
    let error = factory(Rc::new(opts), Rc::new(rt.this.clone()), BaseUrl::History).unwrap_err();
    assert!(error.value.strict_equals(&original));
    assert_eq!(
        error.value.get("units").unwrap().as_str().unwrap().as_ref(),
        &[0xd800]
    );
    assert_eq!(
        error
            .value
            .get("number")
            .unwrap()
            .as_f64()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    assert!(h.http.requests.borrow().is_empty());
}

#[test]
fn rate_limit_alias_and_constructor_enforce_source_five_request_window() {
    for name in [ExchangeName::Kraken, ExchangeName::Krakenspot] {
        let h = harness(name.clone(), options(), |http| {
            http.ok(
                "GET",
                "/v3/accounts",
                serde_json::json!({"result":"success"}),
            );
        });
        let rt = &h.exchange.rt;
        let factory: Factory<'_, Rt> = Rc::new(move |options, receiver, base| {
            ConstructorHost::create_api(
                rt,
                if name == ExchangeName::Kraken {
                    Venue::Futures
                } else {
                    Venue::Spot
                },
                options,
                receiver,
                base,
            )
        });
        let rate_limit: RateLimit<'_, Rt> =
            Rc::new(move |client| ConstructorHost::rate_limit(rt, client, 5.0));
        let independently_typed: &Rc<dyn Fn(Rc<Js>) -> Result<Rc<Js>, Thrown> + '_> = &rate_limit;
        let plain = factory(
            Rc::new(rt.this.get("options").unwrap()),
            Rc::new(rt.this.clone()),
            BaseUrl::Derivatives,
        )
        .unwrap();
        let limited: Handle<Rt> = independently_typed(plain.clone()).unwrap();
        assert!(
            Rc::ptr_eq(&plain, &limited),
            "rate limiter retains the original native client"
        );
        for client in [(*limited).clone(), rt.this.get("xhr").unwrap()] {
            let before = h.http.requests.borrow().len();
            let mut observers = vec![];
            for _ in 0..6 {
                let promise = client
                    .call_method("get", vec![Js::str("/v3/accounts")])
                    .unwrap();
                observers.push(Box::pin(await_js(&rt.exchange_runtime(), promise)));
            }
            h.runtime.pump();
            assert_eq!(
                h.http.requests.borrow().len() - before,
                5,
                "exactly five signed requests before window expiry"
            );
            assert!(matches!(
                observers[5]
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop())),
                Poll::Pending
            ));
            h.runtime.advance(999.0);
            assert_eq!(h.http.requests.borrow().len() - before, 5);
            h.runtime.advance(1.0);
            assert_eq!(h.http.requests.borrow().len() - before, 6);
            for observer in observers {
                h.drive(Box::pin(async {
                    observer.await.map_err(|e| e.to_exchange_error())
                }))
                .unwrap();
            }
            h.runtime.advance(1000.0);
        }
        // Separate unlimitedXHR is not accidentally configured through RateLimit.
        let before = h.http.requests.borrow().len();
        let unlimited = rt.this.get("unlimitedXHR").unwrap();
        let mut promises = vec![];
        for _ in 0..6 {
            promises.push(
                unlimited
                    .call_method("get", vec![Js::str("/v3/accounts")])
                    .unwrap(),
            );
        }
        h.runtime.pump();
        // No clock advance: a hidden five-request window would leave one held.
        assert_eq!(h.http.requests.borrow().len() - before, 6);
        for promise in promises {
            settle(&h, promise);
        }
        assert_eq!(h.http.requests.borrow().len() - before, 6);
    }
}
