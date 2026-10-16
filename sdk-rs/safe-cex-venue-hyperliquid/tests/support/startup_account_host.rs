use super::{
    account,
    cache::{N, eq, object, s},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    spot_account_reads::*,
    spot_json::JsString,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
#[derive(Clone)]
pub struct Host {
    pub base: account::Host,
    pub state: Rc<RefCell<State>>,
}
pub struct State {
    pub effects: Vec<Vec<N>>,
    pub channels: Vec<&'static str>,
    pub trace: Vec<String>,
    pub marker: N,
    pub log_error: N,
    pub data: N,
    pub family: &'static str,
    pub scene: &'static str,
    pub posted: usize,
    pub logs: usize,
    pub info: N,
    pub original_options: N,
    pub expected_user: N,
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(task)
    }
    fn settle_return(&self, result: Result<N, N>) -> AccountFuture<N, N> {
        self.base.settle_return(result)
    }
}
impl WsPingHost for Host {
    type Value = N;
    type Error = N;
    fn undefined(&self) -> N {
        WsPingHost::undefined(&self.base)
    }
    fn literal(&self, t: &JsString) -> N {
        WsPingHost::literal(&self.base, t)
    }
    fn truthy(&self, v: &N) -> bool {
        WsPingHost::truthy(&self.base, v)
    }
    fn global(&mut self, k: &str) -> Result<N, N> {
        WsPingHost::global(&mut self.base, k)
    }
    fn get(&mut self, v: &N, k: &str) -> Result<N, N> {
        if eq(v, &self.base.exchange) && k == "withStartupTimeout" {
            self.state.borrow_mut().trace.push("timeout-get".into());
        }
        let info = self.state.borrow().info.clone();
        if eq(v, &info) && k == "USER_ROLE" && self.state.borrow().scene == "type-mutates-key" {
            let options = account::get(&self.base.exchange, "options");
            self.base.set(&options, "key", &s("new-key"))?;
        }
        self.base.get(v, k)
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::set(&mut self.base, v, k, value)
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::define_data(&mut self.base, v, k, value)
    }
    fn object(&mut self) -> Result<N, N> {
        WsPingHost::object(&mut self.base)
    }
    fn callback(&mut self, capture: PingCallback<N>) -> Result<N, N> {
        WsPingHost::callback(&mut self.base, capture)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            return self.base.call(f, r, args);
        };
        let scene = self.state.borrow().scene;
        match id.as_str() {
            "primary-post" => {
                assert!(
                    eq(r, &account::get(&self.base.exchange, "infoXhr")),
                    "BOOT native original HTTP receiver"
                );
                assert_value_local(&args[0], &s("/info"), "BOOT native endpoint");
                let family = self.state.borrow().family;
                assert_value_local(
                    &account::get(&args[1], "type"),
                    &s(match family {
                        "mode" => "userAbstraction",
                        "role" => "userRole",
                        _ => "spotClearinghouseState",
                    }),
                    "BOOT native request type",
                );
                assert_value_local(
                    &account::get(&args[1], "user"),
                    &self.state.borrow().expected_user,
                    "BOOT native fresh request user",
                );
                assert_value_local(
                    &account::get(&account::get(&args[2], "headers"), "Content-Type"),
                    &s("application/json"),
                    "BOOT native request header",
                );
                self.state.borrow_mut().posted += 1;
                self.state.borrow_mut().trace.push("post".into());
                if scene == "sync-throw" {
                    return Err(self.state.borrow().marker.clone());
                }
                let result = if matches!(scene, "post-error" | "log-error") {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(object(vec![("data", self.state.borrow().data.clone())]))
                };
                Ok(self.base.spawn_native(Box::pin(std::future::ready(result))))
            }
            "primary-timeout" => {
                assert!(eq(r, &self.base.exchange), "BOOT native timeout receiver");
                let family = self.state.borrow().family;
                assert_eq!(args.len(), 4, "BOOT independent timeout argument count");
                assert_value_local(
                    &args[1],
                    &N::Num(if family == "mode" { 11. } else { 13. }),
                    "BOOT native timeout duration",
                );
                assert_value_local(
                    &args[2],
                    &s(if family == "mode" {
                        "userAbstraction resolution"
                    } else {
                        "userRole resolution"
                    }),
                    "BOOT native timeout operation name",
                );
                assert_value_local(
                    &args[3],
                    &if family == "mode" {
                        s("standard")
                    } else {
                        N::Null
                    },
                    "BOOT native timeout fallback",
                );
                if scene == "timeout" {
                    Ok(self
                        .base
                        .spawn_native(Box::pin(std::future::ready(Ok(args[3].clone())))))
                } else {
                    Ok(args[0].clone())
                }
            }
            "primary-log" | "primary-emit" => {
                assert!(
                    eq(r, &self.base.exchange),
                    "BOOT native diagnostic receiver"
                );
                self.state.borrow_mut().effects.push(args.to_vec());
                self.state
                    .borrow_mut()
                    .channels
                    .push(if id == "primary-log" { "log" } else { "emit" });
                self.state.borrow_mut().logs += 1;
                if scene == "log-error" {
                    return Err(self.state.borrow().log_error.clone());
                }
                Ok(N::U)
            }
            "primary-track" => Ok(N::B(scene != "no-track")),
            "primary-use" => Ok(N::B(scene == "portfolioMargin")),
            "primary-update" => {
                assert!(eq(r, &self.base.exchange), "BOOT native update receiver");
                assert_eq!(args.len(), 2, "BOOT independent update argument count");
                assert_value_local(&args[1], &s("rest"), "BOOT native update source");
                let data = self.state.borrow().data.clone();
                if self.truthy(&data) {
                    assert!(
                        eq(&args[0], &data),
                        "BOOT native original collateral object"
                    )
                } else {
                    assert_value_local(
                        &account::get(&args[0], "balances"),
                        &super::cache::arr(vec![]),
                        "BOOT native fresh empty collateral balances",
                    )
                }
                self.state.borrow_mut().effects.push(args.to_vec());
                self.state.borrow_mut().channels.push("update");
                if scene == "handler-error" {
                    Err(self.state.borrow().marker.clone())
                } else {
                    Ok(N::U)
                }
            }
            _ => self.base.call(f, r, args),
        }
    }
}
impl SpotAccountHost for Host {
    fn null(&self) -> N {
        SpotAccountHost::null(&self.base)
    }
    fn boolean(&self, v: bool) -> N {
        SpotAccountHost::boolean(&self.base, v)
    }
    fn number(&self, v: f64) -> N {
        SpotAccountHost::number(&self.base, v)
    }
    fn is_nullish(&self, v: &N) -> bool {
        SpotAccountHost::is_nullish(&self.base, v)
    }
    fn is_undefined(&self, v: &N) -> bool {
        SpotAccountHost::is_undefined(&self.base, v)
    }
    fn is_array(&mut self, v: &N) -> Result<bool, N> {
        SpotAccountHost::is_array(&mut self.base, v)
    }
    fn strict_equal(&self, a: &N, b: &N) -> bool {
        SpotAccountHost::strict_equal(&self.base, a, b)
    }
    fn imported(&mut self, k: &str) -> Result<N, N> {
        match k {
            "INFO_TYPES" => Ok(self.state.borrow().info.clone()),
            "HyperliquidExchange" => Ok(object(vec![
                ("USER_ABSTRACTION_RESOLVE_TIMEOUT_MS", N::Num(11.)),
                ("USER_ROLE_RESOLVE_TIMEOUT_MS", N::Num(13.)),
                ("SPOT_COLLATERAL_REFRESH_INTERVAL_MS", N::Num(10000.)),
            ])),
            _ => self.base.imported(k),
        }
    }
    fn array(&mut self, v: &[N]) -> Result<N, N> {
        SpotAccountHost::array(&mut self.base, v)
    }
    fn spread_array(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::spread_array(&mut self.base, v)
    }
    fn spread_into(&mut self, target: &N, source: &N) -> Result<(), N> {
        SpotAccountHost::spread_into(&mut self.base, target, source)
    }
    fn template_string(&mut self, v: &N) -> Result<JsString, N> {
        SpotAccountHost::template_string(&mut self.base, v)
    }
    fn binary(&mut self, op: &str, a: &N, b: &N) -> Result<N, N> {
        SpotAccountHost::binary(&mut self.base, op, a, b)
    }
    fn interval_binding(&mut self, v: &N) -> Result<[N; 3], N> {
        SpotAccountHost::interval_binding(&mut self.base, v)
    }
    fn pair_binding(&mut self, v: &N) -> Result<[N; 2], N> {
        SpotAccountHost::pair_binding(&mut self.base, v)
    }
    fn index(&mut self, v: &N, k: &N) -> Result<N, N> {
        SpotAccountHost::index(&mut self.base, v, k)
    }
    fn digit_regex(&mut self) -> Result<N, N> {
        SpotAccountHost::digit_regex(&mut self.base)
    }
    fn response_data(&mut self, v: &N) -> Result<N, N> {
        SpotAccountHost::response_data(&mut self.base, v)
    }
    fn candle_binding(&mut self, v: &N) -> Result<[N; 6], N> {
        SpotAccountHost::candle_binding(&mut self.base, v)
    }
    fn construct(&mut self, constructor: &N, args: &[N]) -> Result<N, N> {
        SpotAccountHost::construct(&mut self.base, constructor, args)
    }
    fn throw_value(&self, v: N) -> N {
        SpotAccountHost::throw_value(&self.base, v)
    }
    fn error_value(&self, e: &N) -> N {
        SpotAccountHost::error_value(&self.base, e)
    }
    fn callback(&mut self, c: AccountCallback<N>) -> Result<N, N> {
        SpotAccountHost::callback(&mut self.base, c)
    }
    fn await_value(&mut self, v: N) -> AccountFuture<N, N> {
        SpotAccountHost::await_value(&mut self.base, v)
    }
    fn spawn_native(&self, body: AccountFuture<N, N>) -> N {
        SpotAccountHost::spawn_native(&self.base, body)
    }
}

pub fn assert_value_local(actual: &N, expected: &N, label: &str) {
    match (actual, expected) {
        (N::Num(a), N::Num(b)) => assert_eq!(a.to_bits(), b.to_bits(), "{label}"),
        (N::A(a), N::A(b)) => {
            let a = a.borrow();
            let b = b.borrow();
            assert_eq!(a.len(), b.len(), "{label}");
            for (a, b) in a.iter().zip(b.iter()) {
                assert_value_local(a, b, label)
            }
        }
        _ => assert!(
            eq(actual, expected),
            "{label}: actual={actual:?}, expected={expected:?}"
        ),
    }
}

impl Host {
    pub fn new(family: &'static str, scene: &'static str, data: N) -> Self {
        let enums = object(vec![(
            "LogSeverity",
            object(vec![
                ("Info", s("info")),
                ("Debug", s("debug")),
                ("Warning", s("warning")),
            ]),
        )]);
        let mut base =
            account::Host::new(object(vec![]), enums, object(vec![("INFO", s("/info"))]));
        let e = base.exchange.clone();
        let options = account::get(&e, "options");
        base.set(
            &options,
            "vault",
            &if family == "role" { s("") } else { s("vault") },
        )
        .unwrap();
        base.set(&options, "applicationId", &s("")).unwrap();
        if scene == "no-user" {
            base.set(&options, "vault", &s("")).unwrap();
            base.set(&options, "key", &s("")).unwrap()
        }
        if scene == "application-only" {
            base.set(&options, "vault", &s("")).unwrap();
            base.set(&options, "applicationId", &s("app-master"))
                .unwrap();
        }
        if scene == "existing-vault" {
            base.set(&options, "vault", &s("saved-vault")).unwrap();
        }
        if scene == "existing" {
            base.set(&options, "applicationId", &s("existing")).unwrap()
        }
        for (k, id) in [
            ("infoXhr", "primary-post"),
            ("withStartupTimeout", "primary-timeout"),
            ("log", "primary-log"),
            ("emitAndLogError", "primary-emit"),
            ("shouldTrackSpotHoldings", "primary-track"),
            ("shouldUseSpotCollateralBalance", "primary-use"),
            ("handleSpotClearinghouseStateUpdate", "primary-update"),
        ] {
            base.set(
                &e,
                k,
                &if k == "infoXhr" {
                    object(vec![("post", N::Builtin(id.into()))])
                } else {
                    N::Builtin(id.into())
                },
            )
            .unwrap()
        }
        base.set(
            &e,
            "lastSpotCollateralRefreshAt",
            &N::Num(if ["throttle", "force"].contains(&scene) {
                99999.
            } else {
                0.
            }),
        )
        .unwrap();
        base.set(&e, "spotCollateralRefreshInFlight", &N::Null)
            .unwrap();
        let info = object(vec![
            ("USER_ROLE", s("userRole")),
            ("USER_ABSTRACTION", s("userAbstraction")),
            ("SPOT_CLEARINGHOUSE_STATE", s("spotClearinghouseState")),
        ]);
        let expected_user = if family == "role" {
            s(if scene == "type-mutates-key" {
                "new-key"
            } else {
                "key"
            })
        } else {
            s(if scene == "application-only" {
                "app-master"
            } else {
                "vault"
            })
        };
        let marker = object(vec![
            ("name", s("MarkerError")),
            ("message", s("native marker")),
            ("response", object(vec![("status", N::Num(429.))])),
        ]);
        Self {
            base,
            state: Rc::new(RefCell::new(State {
                effects: vec![],
                channels: vec![],
                trace: vec![],
                marker,
                log_error: object(vec![("name", s("LogMarker")), ("message", s("log marker"))]),
                data,
                family,
                scene,
                posted: 0,
                logs: 0,
                info,
                original_options: options,
                expected_user,
            })),
        }
    }
}
