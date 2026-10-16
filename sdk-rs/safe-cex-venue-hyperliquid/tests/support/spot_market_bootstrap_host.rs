use super::{
    account,
    cache::{self, N, arr, eq, object, s},
};
use safe_cex_venue_hyperliquid::{
    helper_tasks::HelperTaskHost,
    spot_account_reads::*,
    spot_json::JsString,
    spot_market_bootstrap::SpotMarketBootstrapHost,
    ws_ping_lifecycle::{PingCallback, WsPingHost},
};
use std::{cell::RefCell, future::Future, pin::Pin, rc::Rc};
#[derive(Clone)]
pub struct Host {
    pub base: account::Host,
    pub state: Rc<RefCell<State>>,
}
pub struct State {
    pub scene: String,
    pub effects: Vec<N>,
    pub data: N,
    pub marker: N,
    pub key_object: N,
    pub key_marker: N,
    pub rhs_marker: N,
    pub token_reads: usize,
    pub precision_calls: usize,
    pub asset_reads: usize,
    pub settlements: Vec<Result<N, N>>,
    pub proto: N,
    pub dynamic: Vec<(N, Vec<u16>, N)>,
    pub posts: usize,
    pub created_arrays: Vec<N>,
}
pub fn get(v: &N, k: &str) -> N {
    account::get(v, k)
}
fn units(v: &N) -> Vec<u16> {
    match v {
        N::S(t) => t.0.clone(),
        N::Num(n) => safe_cex_num::js_number_string(*n).encode_utf16().collect(),
        N::U => "undefined".encode_utf16().collect(),
        _ => panic!("fixture property key {v:?}"),
    }
}
impl Host {
    pub fn e(&self) -> N {
        self.base.exchange.clone()
    }
    pub fn scene(&self) -> String {
        self.state.borrow().scene.clone()
    }
    pub fn record(&self, n: &str, args: &[N]) {
        let mut values = vec![s(n)];
        values.extend_from_slice(args);
        self.state.borrow_mut().effects.push(arr(values))
    }
    pub fn ready(&self, v: Result<N, N>) -> N {
        self.base.spawn_native(Box::pin(std::future::ready(v)))
    }
    pub fn lookup(&mut self, object: &N, key: &[u16]) -> Result<N, N> {
        if self.scene() == "asset-rhs-before-key"
            && eq(object, &get(&self.state.borrow().data, "1"))
            && key == [48]
        {
            let n = {
                let mut st = self.state.borrow_mut();
                st.asset_reads += 1;
                st.asset_reads
            };
            self.record("asset-read", &[N::Num(n as f64)]);
            if n == 2 {
                return Err(self.state.borrow().rhs_marker.clone());
            }
        }

        let value = self
            .state
            .borrow()
            .dynamic
            .iter()
            .rev()
            .find(|(o, k, _)| eq(o, object) && k == key)
            .map(|(_, _, v)| v.clone());
        if let Some(v) = value {
            return Ok(v);
        }
        match String::from_utf16(key) {
            Ok(key) => self.get(object, &key),
            Err(_) => {
                let prototype = if let N::O(o) = object {
                    o.borrow().prototype.clone()
                } else {
                    None
                };
                if let Some(p) = prototype {
                    self.lookup(&p, key)
                } else {
                    Ok(N::U)
                }
            }
        }
    }
}
impl HelperTaskHost<N, N> for Host {
    fn start_task(&self, task: Pin<Box<dyn Future<Output = ()>>>) {
        self.base.start_task(task)
    }
    fn settle_return(&self, r: Result<N, N>) -> AccountFuture<N, N> {
        let task = self.base.settle_return(r);
        let state = self.state.clone();
        Box::pin(async move {
            let value = task.await;
            state.borrow_mut().settlements.push(value.clone());
            value
        })
    }
}
impl SpotMarketBootstrapHost for Host {
    fn property_key(&mut self, v: &N) -> Result<N, N> {
        if eq(v, &self.state.borrow().key_object) {
            return Err(self.state.borrow().key_marker.clone());
        }
        Ok(N::S(JsString(units(v))))
    }
    fn set_key(&mut self, o: &N, k: &N, v: &N) -> Result<(), N> {
        let key = units(k);
        if key == "__proto__".encode_utf16().collect::<Vec<_>>() {
            let N::O(o) = o else {
                panic!("prototype assignment")
            };
            o.borrow_mut().prototype = Some(v.clone());
            return Ok(());
        }
        if let Ok(key) = String::from_utf16(&key) {
            self.set(o, &key, v)
        } else {
            self.state
                .borrow_mut()
                .dynamic
                .push((o.clone(), key, v.clone()));
            Ok(())
        }
    }
    fn unary_plus(&mut self, v: &N) -> Result<N, N> {
        Ok(N::Num(cache::num(v)))
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
        Ok(match k {
            "Set" | "parseInt" => N::Builtin(k.into()),
            "Number" => object(vec![("MAX_SAFE_INTEGER", N::Num(9007199254740991.))]),
            _ => return self.base.global(k),
        })
    }

    fn get(&mut self, o: &N, k: &str) -> Result<N, N> {
        if self.scene() == "token-rhs-before-key"
            && k == "tokens"
            && eq(o, &get(&self.state.borrow().data, "0"))
        {
            let n = {
                let mut st = self.state.borrow_mut();
                st.token_reads += 1;
                st.token_reads
            };
            self.record("token-read", &[N::Num(n as f64)]);
            if n == 3 {
                return Err(self.state.borrow().rhs_marker.clone());
            }
        }

        if k == "__proto__" {
            if let N::O(r) = o {
                let r = r.borrow();
                if !r.fields.iter().any(|(key, _, _)| key == k) {
                    return Ok(r
                        .prototype
                        .clone()
                        .unwrap_or_else(|| self.state.borrow().proto.clone()));
                }
            }
        }
        self.base.get(o, k)
    }
    fn set(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::set(&mut self.base, v, k, value)
    }
    fn define_data(&mut self, v: &N, k: &str, value: &N) -> Result<(), N> {
        WsPingHost::define_data(&mut self.base, v, k, value)
    }
    fn object(&mut self) -> Result<N, N> {
        let v = object(vec![]);
        let N::O(o) = &v else { unreachable!() };
        o.borrow_mut().prototype = Some(self.state.borrow().proto.clone());
        Ok(v)
    }

    fn callback(&mut self, capture: PingCallback<N>) -> Result<N, N> {
        WsPingHost::callback(&mut self.base, capture)
    }
    fn call(&mut self, f: &N, r: &N, args: &[N]) -> Result<N, N> {
        let N::Builtin(id) = f else {
            panic!("callee {f:?}")
        };
        let first = args.first().cloned().unwrap_or(N::U);
        let scene = self.scene();
        match id.as_str() {
            "post" => {
                self.record("post", args);
                self.state.borrow_mut().posts += 1;
                return Ok(
                    self.ready(if ["post-error", "log-error"].contains(&scene.as_str()) {
                        Err(self.state.borrow().marker.clone())
                    } else {
                        Ok(if scene == "null-response" {
                            N::Null
                        } else {
                            object(vec![("data", self.state.borrow().data.clone())])
                        })
                    }),
                );
            }
            "push" => {
                let N::A(a) = r else { panic!("push") };
                a.borrow_mut().extend_from_slice(args);
                return Ok(N::Num(a.borrow().len() as f64));
            }
            "add" => {
                let N::Set(set) = r else { panic!("add") };
                if !set.borrow().iter().any(|v| eq(v, &first)) {
                    set.borrow_mut().push(first)
                }
                return Ok(r.clone());
            }
            "safeString" => {
                self.record("safeString", &[get(&first, "index"), args[1].clone()]);
                return Ok(get(&first, "szDecimals"));
            }
            "v" => {
                self.record("v", &[get(&first, "coin"), args[1].clone()]);
                let N::S(key) = &args[1] else { panic!("key") };
                return self.get(&first, &String::from_utf16(&key.0).unwrap());
            }
            "parseInt" => {
                let N::S(t) = &first else {
                    panic!("parseInt input")
                };
                let text = String::from_utf16(&t.0).unwrap();
                let number = text.parse::<i32>().map(f64::from).unwrap_or(f64::NAN);
                return Ok(N::Num(number));
            }
            "calculatePricePrecision" => {
                self.record(id, args);
                return Ok(N::Num(if scene == "zero-precision" { 0. } else { 0.01 }));
            }
            "numberToString" => {
                self.record(id, args);
                return Ok(s(&safe_cex_num::js_number_string(cache::num(&first))));
            }
            "parseFiniteNumber" => {
                self.record(id, args);
                let n = cache::num(&first);
                return Ok(N::Num(if n.is_finite() { n } else { 0. }));
            }
            "parseHyperliquidVolumeFields" => {
                self.record(id, &[get(&first, "coin"), args[1].clone()]);
                return Ok(object(vec![
                    (
                        "volume",
                        N::Num(if scene == "nonfinite-volume" {
                            f64::NAN
                        } else {
                            2.
                        }),
                    ),
                    (
                        "quoteVolume",
                        N::Num(if scene == "nonfinite-volume" {
                            f64::INFINITY
                        } else {
                            7.
                        }),
                    ),
                ]));
            }
            "parsePrecision" => {
                self.record(id, args);
                let count = {
                    let mut st = self.state.borrow_mut();
                    st.precision_calls += 1;
                    st.precision_calls
                };
                if scene == "helper-error" || (scene == "late-helper-error" && count == 4) {
                    return Err(self.state.borrow().marker.clone());
                }
                return Ok(N::Num(if scene == "zero-precision" {
                    0.
                } else {
                    cache::num(&first)
                }));
            }
            "calculatePriceChangePercentage" => {
                self.record(id, args);
                return Ok(N::Num(cache::num(&first) - cache::num(&args[1])));
            }
            "resolveSpotDisplayName" => {
                let N::Set(names) = &args[1] else {
                    panic!("token names")
                };
                assert_eq!(names.borrow().len(), 3, "native complete token name set");
                for (i, name) in ["UBTC", "USDC", "USDH"].iter().enumerate() {
                    assert!(
                        eq(&names.borrow()[i], &s(name)),
                        "native exact token name Set member {i}"
                    );
                }
                self.record("display", &[get(&first, "index")]);
                return Ok(
                    if scene != "no-alias" && eq(&get(&first, "name"), &s("UBTC")) {
                        s("BTC")
                    } else {
                        get(&first, "name")
                    },
                );
            }
            "log" => {
                self.record(id, args);
                if scene == "log-error" {
                    return Err(self.state.borrow().marker.clone());
                }
                return Ok(N::U);
            }
            _ => return self.base.call(f, r, args),
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
        Ok(match k {
            "SPOT_ASSET_INDEX_OFFSET" => N::Num(10000.),
            "SPOT_MAX_PRICE_DECIMALS" => N::Num(8.),
            "ENDPOINTS" => object(vec![("INFO", s("/info"))]),
            "INFO_TYPES" => object(vec![(
                "SPOT_META_AND_ASSET_CTXS",
                s("spotMetaAndAssetCtxs"),
            )]),
            _ => return self.base.imported(k),
        })
    }
    fn array(&mut self, v: &[N]) -> Result<N, N> {
        let value = SpotAccountHost::array(&mut self.base, v)?;
        self.state.borrow_mut().created_arrays.push(value.clone());
        Ok(value)
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
        Ok(if op == "+" {
            if matches!(a, N::S(_)) {
                let mut t = units(a);
                t.extend(units(b));
                N::S(JsString(t))
            } else {
                N::Num(cache::num(a) + cache::num(b))
            }
        } else {
            return self.base.binary(op, a, b);
        })
    }

    fn interval_binding(&mut self, v: &N) -> Result<[N; 3], N> {
        SpotAccountHost::interval_binding(&mut self.base, v)
    }
    fn pair_binding(&mut self, v: &N) -> Result<[N; 2], N> {
        SpotAccountHost::pair_binding(&mut self.base, v)
    }
    fn index(&mut self, o: &N, k: &N) -> Result<N, N> {
        let key = self.property_key(k)?;
        self.lookup(o, &units(&key))
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
    fn construct(&mut self, c: &N, args: &[N]) -> Result<N, N> {
        if matches!(c,N::Builtin(n)if n=="Set") {
            assert!(args.is_empty());
            Ok(N::Set(Rc::new(RefCell::new(vec![]))))
        } else {
            self.base.construct(c, args)
        }
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
