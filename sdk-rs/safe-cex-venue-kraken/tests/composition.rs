include!("support/composition_values.rs");
use safe_cex_venue_kraken::private_accounts::{self as account, AccountHost};
use safe_cex_venue_kraken::{
    classes::{self, BoundMember, ClassHost, Initializer, Kind, NativeMember, Return},
    construction::{BaseUrl, ConstructorHost, Factory, RateLimit, Venue},
    ohlcv_polling::{self, PollCallback, PollFuture, PollHost, Slot},
};
struct Callable {
    name: RefCell<String>,
    call: BoundMember<Host>,
}
impl std::fmt::Debug for Callable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("native callable")
    }
}
fn host(cfg: &Value) -> Rc<Host> {
    let h = Rc::new(Host::new(cfg));
    *h.self_ref.borrow_mut() = Some(Rc::downgrade(&h));
    h
}
fn empty() -> Rc<Raw> {
    Rc::new(Raw::Undefined)
}
fn class_name(kind: Kind) -> &'static str {
    match kind {
        Kind::FuturesExchange => "KrakenExchange",
        Kind::SpotExchange => "KrakenSpotExchange",
        Kind::FuturesPublic => "KrakenPublicWebsocket",
        Kind::SpotPublic => "KrakenSpotPublicWebsocket",
        Kind::FuturesPrivate => "KrakenPrivateWebsocket",
        Kind::SpotPrivate => "KrakenSpotPrivateWebsocket",
    }
}
impl Host {
    fn effect(&self, kind: &str, mut pairs: Vec<(&str, Rc<Raw>)>) -> Result<(), E> {
        pairs.insert(0, ("kind", text(kind)));
        self.event(pairs);
        if self.strict_eq(&get(&self.cfg, "effectFail"), &text(kind)) {
            Err(self.original.clone())
        } else {
            Ok(())
        }
    }
    fn rc(&self) -> Rc<Self> {
        self.self_ref.borrow().as_ref().unwrap().upgrade().unwrap()
    }
    fn initialized(&self, receiver: &Raw) -> bool {
        let kind = if flag(&self.cfg, "spot") {
            Kind::SpotExchange
        } else {
            Kind::FuturesExchange
        };
        classes::definitions(kind)
            .iter()
            .filter(|d| !matches!(d.initializer, Initializer::Prototype(_)))
            .all(|d| match receiver {
                Raw::Object(o) => o.borrow().iter().any(|(k, _)| *k == units(d.name)),
                _ => false,
            })
    }
    fn own_define(&self, receiver: &Raw, name: &[u16], value: Rc<Raw>) -> Result<(), E> {
        if let Raw::Function(f) = &*value {
            if f.name.borrow().is_empty() {
                *f.name.borrow_mut() = String::from_utf16(name).unwrap();
            }
        }
        self.assign(receiver, name, value)
    }
    fn call(&self, receiver: Rc<Raw>, name: &str, args: Vec<Rc<Raw>>) -> Result<Return<Host>, E> {
        let value = get(&receiver, name);
        if let Raw::Function(f) = &*value {
            return (f.call)(args);
        }
        if let Some((_, _, body)) = self.prototypes.borrow().iter().find(|(_, n, _)| n == name) {
            return body(receiver, args);
        }
        Err(self.error_text(format!("{name} is not a function")))
    }
    fn drive(&self) -> Result<(), E> {
        for _ in 0..20 {
            let jobs = std::mem::take(&mut *self.pending.borrow_mut());
            if jobs.is_empty() {
                return Ok(());
            }
            for mut future in jobs {
                match future
                    .as_mut()
                    .poll(&mut Context::from_waker(Waker::noop()))
                {
                    Poll::Ready(result) => {
                        if let Err(error) = result {
                            self.effect(
                                "unhandled",
                                vec![(
                                    "identity",
                                    self.boolean(Rc::ptr_eq(&error, &self.original)),
                                )],
                            )?;
                        }
                    }
                    Poll::Pending => self.pending.borrow_mut().push(future),
                }
            }
        }
        Err(self.error_text("continuation floor exceeded".into()))
    }
}
impl ClassHost for Host {
    fn super_socket(&self, kind: Kind, parent: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.effect(
            "superSocket",
            vec![(
                "parentIdentity",
                self.boolean(Rc::ptr_eq(&parent, &get(&self.cfg, "parent"))),
            )],
        )?;
        let id = self.receiver_count.get() + 1;
        self.receiver_count.set(id);
        let receiver = self.object(vec![
            (units("baseId"), self.number(id as f64)),
            (units("parent"), parent.clone()),
            (units("store"), get(&parent, "store")),
            (units("isDisposed"), self.boolean(false)),
        ])?;
        if let Raw::Object(seed) = &*get(&self.cfg, "seed") {
            for (k, v) in seed.borrow().iter() {
                self.assign(&receiver, k, v.clone())?;
            }
        }
        *self.receiver.borrow_mut() = Some(receiver.clone());
        assert!(matches!(
            kind,
            Kind::FuturesPublic | Kind::SpotPublic | Kind::FuturesPrivate | Kind::SpotPrivate
        ));
        Ok(receiver)
    }
    fn define(&self, receiver: &Raw, name: &[u16], value: Rc<Raw>) -> Result<(), E> {
        self.own_define(receiver, name, value)
    }
    fn undefined_value(&self) -> Rc<Raw> {
        empty()
    }
    fn fresh_map(&self) -> Result<Rc<Raw>, E> {
        Ok(Rc::new(Raw::Map(RefCell::new(vec![]))))
    }
    fn imported(&self, name: &'static str) -> Result<Rc<Raw>, E> {
        Ok(self.imports[if name == "GRAFANA_ENDPOINTS_EVERY" {
            0
        } else {
            assert_eq!(name, "GRAFANA_ENDPOINTS_PERIODIC");
            1
        }]
        .clone())
    }
    fn member(
        &self,
        kind: Kind,
        name: &'static str,
        target: &'static str,
    ) -> Result<NativeMember<Self>, E> {
        if flag(&self.cfg, "missingBindings")
            || self.strict_eq(&get(&self.cfg, "missingMember"), &text(name))
        {
            return Err(self.error(text(&format!(
                "Missing native member binding: {}.{name} -> {target}",
                class_name(kind)
            ))));
        }
        let h = self.rc();
        Ok(Rc::new(move |receiver, args| {
            h.effect(
                "bound",
                vec![
                    ("method", text(name)),
                    ("baseId", get(&receiver, "baseId")),
                    (
                        "receiverIdentity",
                        h.boolean(
                            h.receiver
                                .borrow()
                                .as_ref()
                                .is_some_and(|r| Rc::ptr_eq(r, &receiver)),
                        ),
                    ),
                    (
                        "argumentIdentity",
                        h.boolean(
                            args.first()
                                .is_some_and(|v| h.strict_eq(v, &get(&h.cfg, "argument"))),
                        ),
                    ),
                ],
            )?;
            match name {
                "throttledFetchOrders" => {
                    h.effect("throttle", vec![])?;
                }
                "handleFills" => {
                    safe_cex_venue_kraken::private_accounts::futures_fills(
                        &*h,
                        args.first().cloned().unwrap_or_else(empty),
                    )?;
                }
                "handleExecutions" => {
                    safe_cex_venue_kraken::private_accounts::spot_executions(
                        &*h,
                        args.first().cloned().unwrap_or_else(empty),
                    )?;
                }
                "parseOrder" => {
                    return safe_cex_venue_kraken::private_accounts::spot_parse_order(
                        &*h,
                        args.first().cloned().unwrap_or_else(empty),
                    )
                    .map(Return::Value);
                }
                "dispose" if kind == Kind::FuturesExchange => {
                    safe_cex_venue_kraken::lifecycle::futures_dispose(&DisposeBridge {
                        host: h.clone(),
                        receiver: receiver.clone(),
                    })
                    .map_err(|e| h.error(text(&e.message)))?;
                }
                "tick" if kind == Kind::SpotExchange => {
                    let bridge = Rc::new(TickBridge {
                        host: h.clone(),
                        receiver: receiver.clone(),
                    });
                    let h = h.clone();
                    return Ok(Return::Future(Box::pin(async move {
                        safe_cex_venue_kraken::periodic_sync::spot_tick(bridge, None)
                            .await
                            .map_err(|e| h.error(text(&e.message)))?;
                        Ok(empty())
                    })));
                }
                "onMessage" if kind == Kind::FuturesPublic => {
                    safe_cex_venue_kraken::message_dispatch::futures_public(
                        &MessageBridge {
                            host: h.clone(),
                            receiver: receiver.clone(),
                        },
                        get(&args[0], "data"),
                    )
                    .map_err(|e| h.error(text(&e.message)))?;
                }
                "onMessage" if kind == Kind::SpotPublic => {
                    safe_cex_venue_kraken::message_dispatch::spot_public(
                        &MessageBridge {
                            host: h.clone(),
                            receiver: receiver.clone(),
                        },
                        get(&args[0], "data"),
                    )
                    .map_err(|e| h.error(text(&e.message)))?;
                }
                "onMessage" if kind == Kind::FuturesPrivate => {
                    safe_cex_venue_kraken::message_dispatch::futures_private(
                        &MessageBridge {
                            host: h.clone(),
                            receiver: receiver.clone(),
                        },
                        get(&args[0], "data"),
                    )
                    .map_err(|e| h.error(text(&e.message)))?;
                }
                "onMessage" if kind == Kind::SpotPrivate => {
                    safe_cex_venue_kraken::message_dispatch::spot_private(
                        &MessageBridge {
                            host: h.clone(),
                            receiver: receiver.clone(),
                        },
                        get(&args[0], "data"),
                    )
                    .map_err(|e| h.error(text(&e.message)))?;
                }
                "mapTimeframe" => {
                    let key = h.string(&args[0])?;
                    let hit = safe_cex_venue_kraken::ws_helpers::map_timeframe(&key);
                    if let safe_cex_num::TableHit::Mapped(v) = hit {
                        return Ok(Return::Value(h.number(v)));
                    }
                }
                _ => {}
            }
            Ok(Return::Value(empty()))
        }))
    }
    fn arrow(&self, body: BoundMember<Self>) -> Result<Rc<Raw>, E> {
        Ok(Rc::new(Raw::Function(Rc::new(Callable {
            name: RefCell::new(String::new()),
            call: body,
        }))))
    }
    fn prototype(&self, kind: Kind, name: &'static str, body: NativeMember<Self>) -> Result<(), E> {
        let mut prototypes = self.prototypes.borrow_mut();
        if !prototypes.iter().any(|(k, n, _)| *k == kind && n == name) {
            prototypes.push((kind, name.into(), body));
        }
        Ok(())
    }
}
impl ConstructorHost for Host {
    fn super_exchange(
        &self,
        name: Rc<Raw>,
        options: Rc<Raw>,
        store: Rc<Raw>,
    ) -> Result<Rc<Raw>, E> {
        self.effect(
            "superExchange",
            vec![
                (
                    "nameIdentity",
                    self.boolean(Rc::ptr_eq(&name, &get(&self.cfg, "name"))),
                ),
                (
                    "optionsIdentity",
                    self.boolean(Rc::ptr_eq(&options, &get(&self.cfg, "options"))),
                ),
                (
                    "storeIdentity",
                    self.boolean(Rc::ptr_eq(&store, &self.store)),
                ),
            ],
        )?;
        let id = self.receiver_count.get() + 1;
        self.receiver_count.set(id);
        let receiver = self.object(vec![
            (units("baseId"), self.number(id as f64)),
            (units("exchangeName"), name),
            (units("options"), options),
            (units("store"), store),
            (units("isDisposed"), self.boolean(false)),
            (units("publicWebsocket"), get(&self.cfg, "publicSocket")),
            (units("privateWebsocket"), get(&self.cfg, "privateSocket")),
        ])?;
        if let Raw::Object(seed) = &*get(&self.cfg, "seed") {
            for (k, v) in seed.borrow().iter() {
                self.assign(&receiver, k, v.clone())?;
            }
        }
        *self.receiver.borrow_mut() = Some(receiver.clone());
        Ok(receiver)
    }
    fn secondary_client<'a>(
        &'a self,
        options: Rc<Raw>,
        receiver: Rc<Raw>,
        factory: Factory<'a, Self>,
        limit: RateLimit<'a, Self>,
    ) -> Result<Rc<Raw>, E> {
        self.effect(
            "secondary",
            vec![
                (
                    "optionsIdentity",
                    self.boolean(Rc::ptr_eq(&options, &get(&self.cfg, "options"))),
                ),
                ("initialized", self.boolean(self.initialized(&receiver))),
                (
                    "xhrUndefined",
                    self.boolean(self.undefined(&get(&receiver, "xhr"))),
                ),
            ],
        )?;
        let factory_options = if self.truthy(&get(&self.cfg, "secondaryOptions")) {
            get(&self.cfg, "secondaryOptions")
        } else {
            options
        };
        let client = factory(factory_options, receiver, BaseUrl::Derivatives)?;
        limit(client)
    }
    fn create_api(
        &self,
        venue: Venue,
        options: Rc<Raw>,
        receiver: Rc<Raw>,
        base: BaseUrl,
    ) -> Result<Rc<Raw>, E> {
        let n = self.api_count.get();
        self.api_count.set(n + 1);
        self.effect(
            "api",
            vec![
                ("spot", self.boolean(venue == Venue::Spot)),
                (
                    "base",
                    text(if base == BaseUrl::History {
                        "history"
                    } else {
                        "derivatives"
                    }),
                ),
                ("options", options),
                (
                    "receiverIdentity",
                    self.boolean(
                        self.receiver
                            .borrow()
                            .as_ref()
                            .is_some_and(|r| Rc::ptr_eq(r, &receiver)),
                    ),
                ),
                (
                    "xhrUndefined",
                    self.boolean(self.undefined(&get(&receiver, "xhr"))),
                ),
                (
                    "unlimitedUndefined",
                    self.boolean(self.undefined(&get(&receiver, "unlimitedXHR"))),
                ),
                (
                    "historyUndefined",
                    self.boolean(self.undefined(&get(&receiver, "historyXhr"))),
                ),
                ("id", self.number(n as f64)),
            ],
        )?;
        if self.numeric(&get(&self.cfg, "apiFailAt"))? == n as f64 {
            return Err(self.original.clone());
        }
        self.object(vec![
            (units("client"), self.number(n as f64)),
            (
                units("base"),
                text(if base == BaseUrl::History {
                    "history"
                } else {
                    "derivatives"
                }),
            ),
        ])
    }
    fn rate_limit(&self, client: Rc<Raw>, max_rps: f64) -> Result<Rc<Raw>, E> {
        self.effect(
            "rateLimit",
            vec![("client", client.clone()), ("maxRPS", self.number(max_rps))],
        )?;
        if flag(&self.cfg, "wrapRate") {
            self.object(vec![(units("wrapped"), client)])
        } else {
            Ok(client)
        }
    }
}

fn copy_raw(v: &Raw) -> Rc<Raw> {
    Rc::new(match v {
        Raw::Undefined => Raw::Undefined,
        Raw::Null => Raw::Null,
        Raw::Bool(b) => Raw::Bool(*b),
        Raw::Number(n) => Raw::Number(*n),
        Raw::Text(s) => Raw::Text(s.clone()),
        Raw::Map(m) => Raw::Map(RefCell::new(m.borrow().clone())),
        Raw::Function(f) => Raw::Function(f.clone()),
        Raw::Object(o) => Raw::Object(RefCell::new(o.borrow().clone())),
        Raw::Array(a) => Raw::Array(a.clone()),
    })
}
fn contains(root: &Raw, target: &Raw) -> bool {
    std::ptr::eq(root, target)
        || match root {
            Raw::Object(o) => o.borrow().iter().any(|(_, v)| contains(v, target)),
            Raw::Array(a) => a.iter().any(|v| contains(v, target)),
            _ => false,
        }
}
impl Host {
    fn info_identity(&self, o: &Raw) -> Rc<Raw> {
        self.boolean(contains(&get(&self.cfg, "input"), &get(o, "info")))
    }
    fn update_existing(&self, selector: &Raw, patch: &Raw) -> Result<(), E> {
        for o in self.items(&get(&self.store, "orders"))? {
            if self.strict_eq(&get(&o, "id"), &get(selector, "id")) {
                for (k, v) in self.entries(patch)? {
                    self.assign(&o, &k, v)?;
                }
                break;
            }
        }
        Ok(())
    }
}
impl AccountHost for Host {
    fn add(&self, a: &Raw, b: &Raw) -> Result<Rc<Raw>, E> {
        let primitive = |v: &Raw| {
            if matches!(v, Raw::Object(_) | Raw::Array(_)) {
                self.string(v).map(|s| self.text(&s))
            } else {
                Ok(copy_raw(v))
            }
        };
        let a = primitive(a)?;
        let b = primitive(b)?;
        if matches!(&*a, Raw::Text(_)) || matches!(&*b, Raw::Text(_)) {
            Ok(self.text(&[self.string(&a)?, self.string(&b)?].concat()))
        } else {
            Ok(self.number(self.numeric(&a)? + self.numeric(&b)?))
        }
    }
    fn to_string_method(&self, v: &Raw) -> Result<Rc<Raw>, E> {
        if self.nullish(v) {
            self.field(v, &units("toString"), false)?;
        }
        Ok(self.text(&self.string(v)?))
    }
    fn date_milliseconds(&self, v: &Raw) -> Result<f64, E> {
        let n = match v {
            Raw::Text(s) => safe_cex_num::js_date_parse(&String::from_utf16(s).unwrap(), |n| n),
            _ => self.numeric(v)?,
        };
        Ok(if !n.is_finite() || n.abs() > 8640000000000000.0 {
            f64::NAN
        } else {
            n.trunc() + 0.0
        })
    }
    fn position_id(&self, symbol: &Raw, side: &str) -> Result<Rc<Raw>, E> {
        Ok(self.text(&[self.string(symbol)?, units(":"), units(side)].concat()))
    }
    fn optional_preference(&self, cache: &str, symbol: &Raw) -> Result<Rc<Raw>, E> {
        if flag(
            &self.cfg,
            if cache == "margin" {
                "missingMargin"
            } else {
                "missingLeverage"
            },
        ) {
            Ok(Rc::new(Raw::Undefined))
        } else {
            self.preference(cache, symbol)
        }
    }
    fn optional_set_margin(&self, s: Rc<Raw>, m: Rc<Raw>) -> Result<(), E> {
        if flag(&self.cfg, "missingMargin") {
            Ok(())
        } else {
            self.set_preference(s, m)
        }
    }
    fn update_order(&self, selector: Rc<Raw>, patch: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "updateOrder",
            vec![("selector", selector.clone()), ("patch", patch.clone())],
        )?;
        self.update_existing(&selector, &patch)
    }
    fn order_status(&self, selector: Rc<Raw>, status: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "status",
            vec![("selector", selector.clone()), ("status", status.clone())],
        )?;
        self.update_existing(&selector, &*self.object(vec![(units("status"), status)])?)
    }
    fn add_order(&self, order: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "addOrder",
            vec![
                ("order", order.clone()),
                ("infoIdentity", self.info_identity(&order)),
            ],
        )?;
        let mut rows = self.items(&get(&self.store, "orders"))?;
        if let Some(i) = rows
            .iter()
            .position(|o| self.strict_eq(&get(o, "id"), &get(&order, "id")))
        {
            rows[i] = order;
        } else {
            rows.push(order);
        }
        self.assign(&self.store, &units("orders"), self.array(rows))
    }
    fn full_orders(&self, orders: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "fullOrders",
            vec![
                ("orders", orders.clone()),
                (
                    "infoIdentity",
                    self.boolean(
                        self.items(&orders)?
                            .iter()
                            .all(|o| matches!(&*self.info_identity(o), Raw::Bool(true))),
                    ),
                ),
            ],
        )?;
        self.assign(&self.store, &units("orders"), orders)
    }
    fn full_positions(&self, p: Rc<Raw>) -> Result<(), E> {
        self.effect(
            "fullPositions",
            vec![
                ("positions", p.clone()),
                (
                    "infoIdentity",
                    self.boolean(
                        self.items(&p)?
                            .iter()
                            .all(|o| matches!(&*self.info_identity(o), Raw::Bool(true))),
                    ),
                ),
            ],
        )?;
        self.assign(&self.store, &units("positions"), p)
    }
    fn fill(&self, payload: Rc<Raw>) -> Result<(), E> {
        self.effect("fill", vec![("payload", payload)])
    }
    fn throttle(&self) -> Result<(), E> {
        if let Some(receiver) = self.receiver.borrow().clone() {
            if matches!(&*get(&receiver, "throttledFetchOrders"), Raw::Function(_)) {
                self.call(receiver, "throttledFetchOrders", vec![])?;
                return Ok(());
            }
        }
        self.effect("throttle", vec![])
    }
    fn parse_order(&self, item: Rc<Raw>) -> Result<Rc<Raw>, E> {
        self.effect(
            "parse",
            vec![(
                "identity",
                self.boolean(contains(&get(&self.cfg, "input"), &item)),
            )],
        )?;
        if flag(&self.cfg, "overrideParse") {
            self.object(vec![
                (units("id"), text("override")),
                (units("symbol"), text("Z")),
                (units("info"), item),
            ])
        } else {
            account::spot_parse_order(self, item)
        }
    }
    fn null(&self) -> Rc<Raw> {
        Rc::new(Raw::Null)
    }
}
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_venue_kraken::message_dispatch::MessageHost;
struct MessageBridge {
    host: Rc<Host>,
    receiver: Rc<Raw>,
}
impl MessageBridge {
    fn map<T>(&self, result: Result<T, E>) -> Result<T, ExchangeError> {
        result.map_err(|e| {
            ExchangeError::new(String::from_utf16(&self.host.string(&e.message).unwrap()).unwrap())
        })
    }
}
impl MessageHost for MessageBridge {
    type Value = Raw;
    fn disposed(&self) -> Result<bool, ExchangeError> {
        Ok(self.host.truthy(&get(&self.receiver, "isDisposed")))
    }
    fn track_received(&self) -> Result<(), ExchangeError> {
        self.map(self.host.effect("received", vec![]))
    }
    fn parse_data(&self, data: &Raw, _tolerant: bool) -> Result<Rc<Raw>, ExchangeError> {
        let text = self.map(self.host.string(data))?;
        serde_json::from_str::<Value>(&String::from_utf16(&text).unwrap())
            .map(|v| decode(&v))
            .map_err(|e| ExchangeError::new(e.to_string()))
    }
    fn field(&self, v: &Raw, key: &'static str, optional: bool) -> Result<Rc<Raw>, ExchangeError> {
        self.map(ReadHost::field(&*self.host, v, &units(key), optional))
    }
    fn truthy(&self, v: &Raw) -> bool {
        self.host.truthy(v)
    }
    fn equals(&self, v: &Raw, text_value: &'static str) -> bool {
        self.host.strict_eq(v, &text(text_value))
    }
    fn string(&self, v: &Raw) -> Result<Vec<u16>, ExchangeError> {
        self.map(self.host.string(v))
    }
    fn stringify(&self, v: &Raw) -> Result<Vec<u16>, ExchangeError> {
        Ok(units(&to_value(v).to_string()))
    }
    fn includes(&self, v: &Raw, text: &'static str) -> Result<bool, ExchangeError> {
        self.map(self.host.includes(v, text))
    }
    fn stats(&self, feed: Rc<Raw>) -> Result<(), ExchangeError> {
        self.map(self.host.effect("stats", vec![("feed", feed)]))
    }
    fn invoke(&self, method: &'static str, message: Rc<Raw>) -> Result<(), ExchangeError> {
        let result = self.host.call(self.receiver.clone(), method, vec![message]);
        self.map(result).map(|_| ())
    }
    fn perform_auth(&self) -> Result<(), ExchangeError> {
        let result = self.host.call(self.receiver.clone(), "performAuth", vec![]);
        self.map(result).map(|_| ())
    }
    fn pending_challenge(&self, v: Vec<u16>) -> Result<(), ExchangeError> {
        self.map(self.host.assign(
            &self.receiver,
            &units("pendingChallenge"),
            self.host.text(&v),
        ))
    }
    fn waiting_challenge(&self, v: bool) -> Result<(), ExchangeError> {
        self.map(self.host.assign(
            &self.receiver,
            &units("waitingChallenge"),
            self.host.boolean(v),
        ))
    }
    fn authenticated(&self, v: bool) -> Result<(), ExchangeError> {
        self.map(self.host.assign(
            &self.receiver,
            &units("authenticated"),
            self.host.boolean(v),
        ))
    }
    fn clear_token(&self) -> Result<(), ExchangeError> {
        self.map(self.host.assign(&self.receiver, &units("wsToken"), empty()))
    }
    fn log(&self, message: Vec<u16>, severity: LogSeverity) -> Result<(), ExchangeError> {
        self.map(self.host.effect(
            "log",
            vec![
                ("message", self.host.text(&message)),
                (
                    "severity",
                    text(match severity {
                        LogSeverity::Error => "error",
                        LogSeverity::Warning => "warning",
                        _ => "other",
                    }),
                ),
            ],
        ))
    }
    fn console_subscription(
        &self,
        prefix: &'static str,
        value: Rc<Raw>,
    ) -> Result<(), ExchangeError> {
        self.map(self.host.effect(
            "subscriptionError",
            vec![("prefix", text(prefix)), ("value", value)],
        ))
    }
    fn console_parse(
        &self,
        prefix: &'static str,
        error: ExchangeError,
        _data: Rc<Raw>,
    ) -> Result<(), ExchangeError> {
        self.map(self.host.effect(
            "parseError",
            vec![("prefix", text(prefix)), ("message", text(&error.message))],
        ))
    }
}
fn slot_index(slot: Slot) -> usize {
    match slot {
        Slot::Timeout => 0,
        Slot::Active => 1,
        Slot::Candle => 2,
    }
}
impl PollHost for Host {
    fn map_has(&self, slot: Slot, key: &[u16]) -> Result<bool, E> {
        Ok(self.maps.borrow()[slot_index(slot)]
            .iter()
            .any(|(k, _)| k == key))
    }
    fn map_get(&self, slot: Slot, key: &[u16]) -> Result<Rc<Raw>, E> {
        Ok(self.maps.borrow()[slot_index(slot)]
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
            .unwrap_or_else(empty))
    }
    fn map_set(&self, slot: Slot, key: &[u16], value: Rc<Raw>) -> Result<(), E> {
        let mut maps = self.maps.borrow_mut();
        let rows = &mut maps[slot_index(slot)];
        if let Some(row) = rows.iter_mut().find(|(k, _)| k == key) {
            row.1 = value
        } else {
            rows.push((key.into(), value));
        }
        Ok(())
    }
    fn map_delete(&self, slot: Slot, key: &[u16]) -> Result<(), E> {
        self.maps.borrow_mut()[slot_index(slot)].retain(|(k, _)| k != key);
        Ok(())
    }
    fn clear_timeout(&self, timer: Rc<Raw>) -> Result<(), E> {
        self.effect("clear", vec![("timer", timer.clone())])?;
        for (t, _, canceled) in self.timers.borrow_mut().iter_mut() {
            if self.strict_eq(t, &timer) {
                *canceled = true;
            }
        }
        Ok(())
    }
    fn set_timeout(&self, callback: PollCallback<Self>, delay: f64) -> Result<Rc<Raw>, E> {
        let n = self.timer_count.get() + 1;
        self.timer_count.set(n);
        let timer = self.number(n as f64);
        let same = self
            .timers
            .borrow()
            .last()
            .map(|(_, cb, _)| Rc::ptr_eq(cb, &callback));
        self.effect(
            "timer",
            vec![
                ("timer", timer.clone()),
                ("delay", self.number(delay)),
                ("sameCallback", self.boolean(same.unwrap_or(true))),
            ],
        )?;
        self.timers
            .borrow_mut()
            .push((timer.clone(), callback, false));
        Ok(timer)
    }
    fn fetch_candles(
        &self,
        options: Rc<Raw>,
    ) -> Result<impl Future<Output = Result<Rc<Raw>, E>>, E> {
        let n = self.calls.get();
        self.calls.set(n + 1);
        self.effect("fetch", vec![("options", options)])?;
        if flag(&self.cfg, "fetchSyncFail") {
            return Err(self.original.clone());
        }
        let pages = self.items(&get(&self.cfg, "pages"))?;
        let page = pages[n.min(pages.len() - 1)].clone();
        Ok(async move {
            yield_once().await;
            if flag(&self.cfg, "fetchFail") {
                Err(self.original.clone())
            } else {
                Ok(page)
            }
        })
    }
    fn console_error(&self, prefix: &'static str, error: E) -> Result<(), E> {
        self.effect(
            "console",
            vec![
                ("prefix", text(prefix)),
                ("identity", self.boolean(Rc::ptr_eq(&error, &self.original))),
            ],
        )
    }
    fn spawn(&self, mut future: PollFuture<Self>) {
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(error)) => {
                self.pending
                    .borrow_mut()
                    .push(Box::pin(async move { Err(error) }));
            }
            Poll::Pending => self.pending.borrow_mut().push(future),
        }
    }
}
async fn yield_once() {
    let mut first = true;
    std::future::poll_fn(move |_| {
        if first {
            first = false;
            Poll::Pending
        } else {
            Poll::Ready(())
        }
    })
    .await;
}
struct DisposeBridge {
    host: Rc<Host>,
    receiver: Rc<Raw>,
}
impl DisposeBridge {
    fn rows(&self, name: &str) -> Vec<(Vec<u16>, Rc<Raw>)> {
        match &*get(&self.receiver, name) {
            Raw::Map(rows) => rows.borrow().clone(),
            _ => panic!("native receiver map"),
        }
    }
    fn clear(&self, name: &str) {
        match &*get(&self.receiver, name) {
            Raw::Map(rows) => rows.borrow_mut().clear(),
            _ => panic!("native receiver map"),
        }
    }
}
impl safe_cex_venue_kraken::lifecycle::DisposalHost for DisposeBridge {
    type Timeout = Raw;
    type Key = Raw;
    type Timeouts = std::vec::IntoIter<Result<Rc<Raw>, ExchangeError>>;
    type Keys = std::vec::IntoIter<Result<Rc<Raw>, ExchangeError>>;
    fn polling_timeouts(&self) -> Result<Self::Timeouts, ExchangeError> {
        Ok(self
            .rows("ohlcvPollingTimeouts")
            .into_iter()
            .map(|(_, v)| Ok(v))
            .collect::<Vec<_>>()
            .into_iter())
    }
    fn clear_timeout(&self, timer: &Rc<Raw>) -> Result<(), ExchangeError> {
        self.host.clear_timeout(timer.clone()).map_err(|e| {
            ExchangeError::new(String::from_utf16(&self.host.string(&e.message).unwrap()).unwrap())
        })
    }
    fn active_keys(&self) -> Result<Self::Keys, ExchangeError> {
        Ok(self
            .rows("ohlcvPollingActive")
            .into_iter()
            .map(|(k, _)| Ok(self.host.text(&k)))
            .collect::<Vec<_>>()
            .into_iter())
    }
    fn set_inactive(&self, key: &Rc<Raw>) -> Result<(), ExchangeError> {
        let key = self.host.string(key).unwrap();
        let value = get(&self.receiver, "ohlcvPollingActive");
        if let Raw::Map(rows) = &*value {
            if let Some(row) = rows.borrow_mut().iter_mut().find(|(k, _)| *k == key) {
                row.1 = self.host.boolean(false)
            }
        }
        Ok(())
    }
    fn clear_timeouts(&self) -> Result<(), ExchangeError> {
        self.clear("ohlcvPollingTimeouts");
        Ok(())
    }
    fn clear_active(&self) -> Result<(), ExchangeError> {
        self.clear("ohlcvPollingActive");
        Ok(())
    }
    fn clear_candles(&self) -> Result<(), ExchangeError> {
        self.clear("lastCandleCache");
        Ok(())
    }
    fn dispose_base(&self) -> Result<(), ExchangeError> {
        self.host
            .effect(
                "baseDispose",
                vec![("baseId", get(&self.receiver, "baseId"))],
            )
            .unwrap();
        self.host
            .assign(
                &self.receiver,
                &units("isDisposed"),
                self.host.boolean(true),
            )
            .unwrap();
        Ok(())
    }
}
use safe_cex_exchange::runtime::ExchangeFuture;
use safe_cex_venue_kraken::periodic_sync::{TickCallback, TickHost, TickUpdate};
struct TickBridge {
    host: Rc<Host>,
    receiver: Rc<Raw>,
}
fn outside<T>() -> Result<T, ExchangeError> {
    Err(ExchangeError::new(
        "outside disposed-tick class witness floor",
    ))
}
impl TickHost for TickBridge {
    type Orders = Raw;
    type Positions = Raw;
    type Balance = Raw;
    type Task = ExchangeFuture<'static, ()>;
    fn is_disposed(&self) -> bool {
        self.host.truthy(&get(&self.receiver, "isDisposed"))
    }
    fn fetch_leverage_settings(&self) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        outside()
    }
    fn fetch_orders(&self) -> Result<ExchangeFuture<'static, Rc<Raw>>, ExchangeError> {
        outside()
    }
    fn fetch_positions(&self) -> Result<ExchangeFuture<'static, Rc<Raw>>, ExchangeError> {
        outside()
    }
    fn fetch_balance(&self) -> Result<ExchangeFuture<'static, Rc<Raw>>, ExchangeError> {
        outside()
    }
    fn balance_truthy(&self, v: &Raw) -> bool {
        self.host.truthy(v)
    }
    fn loaded(&self) -> Result<safe_cex_types::StoreDataLoaded, ExchangeError> {
        Ok(safe_cex_types::StoreDataLoaded {
            balance: false,
            orders: false,
            markets: false,
            tickers: false,
            positions: false,
        })
    }
    fn update(&self, _update: TickUpdate<Raw, Raw, Raw>) -> Result<(), ExchangeError> {
        outside()
    }
    fn start_then(&self, task: ExchangeFuture<'static, ()>) -> Result<Self::Task, ExchangeError> {
        Ok(task)
    }
    fn promise_all(
        &self,
        _tasks: Vec<Self::Task>,
    ) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        outside()
    }
    fn emit_tick_error(
        &self,
        _error: ExchangeError,
        _fallback: &'static str,
    ) -> Result<(), ExchangeError> {
        outside()
    }
    fn current_tick(&self, _count: f64) -> Result<ExchangeFuture<'static, ()>, ExchangeError> {
        outside()
    }
    fn schedule_loop(&self, _callback: TickCallback, _delay: f64) -> Result<(), ExchangeError> {
        outside()
    }
}
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .env("TZ", "UTC")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/composition-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let data: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(data["header"]["hashes"].as_object().unwrap().len(), 7);
        assert_eq!(data["header"]["tz"], "UTC");
        data
    })
}
fn native(actual: &Raw, expected: &Value, label: &str) {
    match actual {
        Raw::Undefined => assert_eq!(expected["undefined"], true, "{label}: native undefined"),
        Raw::Null => assert!(expected.is_null(), "{label}: native null"),
        Raw::Bool(v) => assert_eq!(expected.as_bool(), Some(*v), "{label}: native boolean"),
        Raw::Text(v) => assert_eq!(
            expected.as_str().map(units),
            Some(v.clone()),
            "{label}: native UTF16"
        ),
        Raw::Number(v) => {
            let bits = expected["number"]
                .as_str()
                .unwrap_or_else(|| panic!("{label}: expected native number {expected}"));
            let n = f64::from_bits(u64::from_str_radix(bits, 16).unwrap());
            if n.is_nan() {
                assert!(v.is_nan(), "{label}: native NaN")
            } else {
                assert_eq!(v.to_bits(), n.to_bits(), "{label}: native bits")
            }
        }
        Raw::Array(a) => {
            let e = expected.as_array().unwrap();
            assert_eq!(a.len(), e.len(), "{label}: native length");
            for (i, (a, e)) in a.iter().zip(e).enumerate() {
                native(a, e, &format!("{label}[{i}]"));
            }
        }
        Raw::Object(o) => {
            let o = o.borrow();
            let e = expected
                .as_object()
                .unwrap_or_else(|| panic!("{label}: expected object {expected}"));
            assert_eq!(
                o.iter()
                    .map(|(k, _)| String::from_utf16(k).unwrap())
                    .collect::<Vec<_>>(),
                e.keys().cloned().collect::<Vec<_>>(),
                "{label}: native own-key order"
            );
            for (k, a) in o.iter() {
                let k = String::from_utf16(k).unwrap();
                native(a, &e[&k], &format!("{label}.{k}"));
            }
        }
        Raw::Map(_) | Raw::Function(_) => {
            panic!("native class special values require direct shape assertions")
        }
    }
}
fn error(h: &Host, actual: &E, expected: &Value) {
    assert!(
        !expected.is_null(),
        "unexpected native error {}",
        to_value(&actual.message)
    );
    assert_eq!(
        Rc::ptr_eq(actual, &h.original),
        expected["identity"].as_bool().unwrap(),
        "native error allocation"
    );
    assert_eq!(
        actual.name,
        expected["name"].as_str().unwrap(),
        "native error name"
    );
    native(&actual.message, &expected["message"], "native error");
}
fn construct_class(h: &Rc<Host>, kind: Kind) -> Result<Rc<Raw>, E> {
    match kind {
        Kind::FuturesExchange => classes::futures_exchange(
            &**h,
            get(&h.cfg, "name"),
            get(&h.cfg, "options"),
            h.store.clone(),
        ),
        Kind::SpotExchange => classes::spot_exchange(
            &**h,
            get(&h.cfg, "name"),
            get(&h.cfg, "options"),
            h.store.clone(),
        ),
        Kind::FuturesPublic => classes::futures_public(&**h, get(&h.cfg, "parent")),
        Kind::SpotPublic => classes::spot_public(&**h, get(&h.cfg, "parent")),
        Kind::FuturesPrivate => classes::futures_private(&**h, get(&h.cfg, "parent")),
        Kind::SpotPrivate => classes::spot_private(&**h, get(&h.cfg, "parent")),
    }
}
fn setup(cfg: &Value) -> Rc<Host> {
    let h = host(cfg);
    let parent = get(&h.cfg, "parent");
    if let Raw::Object(_) = &*parent {
        h.assign(&parent, &units("store"), h.store.clone()).unwrap();
        h.assign(&parent, &units("options"), get(&h.cfg, "options"))
            .unwrap();
        h.assign(&parent, &units("emitter"), h.object(vec![]).unwrap())
            .unwrap();
    }
    h
}
fn shape(actual: &Raw, expected: &Value, own_floor: usize) {
    let Raw::Object(o) = actual else {
        panic!("native class receiver")
    };
    let o = o.borrow();
    let e = expected.as_array().unwrap();
    assert_eq!(o.len(), e.len(), "native own-key order/count");
    for ((key, value), entry) in o.iter().zip(e) {
        assert_eq!(
            *key,
            units(entry["key"].as_str().unwrap()),
            "native own-key order"
        );
        assert_eq!(entry["enumerable"], true);
        assert_eq!(entry["configurable"], true);
        assert_eq!(entry["writable"], true);
        match &**value {
            Raw::Function(f) => {
                assert_eq!(entry["kind"], "function");
                assert_eq!(
                    *f.name.borrow(),
                    entry["value"].as_str().unwrap(),
                    "native function name"
                );
            }
            Raw::Map(rows) => {
                assert_eq!(entry["kind"], "map");
                assert_eq!(rows.borrow().len(), 0, "native fresh Map floor");
                assert_eq!(entry["value"], 0);
            }
            v => {
                assert_eq!(entry["kind"], "value");
                native(v, &entry["value"], "native class field");
            }
        }
    }
    assert!(o.len() >= own_floor, "independent own field floor");
}
fn settle(result: Return<Host>) -> Result<Rc<Raw>, E> {
    match result {
        Return::Value(v) => Ok(v),
        Return::Future(mut f) => {
            for _ in 0..20 {
                if let Poll::Ready(v) = f.as_mut().poll(&mut Context::from_waker(Waker::noop())) {
                    return v;
                }
            }
            panic!("class continuation floor exceeded");
        }
    }
}
fn class_check(kind: Kind, case_floor: usize, own_floor: usize, arrow_floor: usize) {
    let rows = oracle()["classes"][class_name(kind)].as_array().unwrap();
    assert_eq!(rows.len(), case_floor, "independent class case floor");
    assert_eq!(
        classes::definitions(kind)
            .iter()
            .filter(|d| !matches!(d.initializer, Initializer::Prototype(_)))
            .count(),
        own_floor
    );
    assert_eq!(
        classes::definitions(kind)
            .iter()
            .filter(|d| matches!(d.initializer, Initializer::Arrow(_)))
            .count(),
        arrow_floor
    );
    for row in rows {
        let h = setup(&row["cfg"]);
        let result = construct_class(&h, kind);
        if flag(&h.cfg, "missingBindings") {
            let err = result.unwrap_err();
            assert_eq!(
                row["sourceConstructed"], true,
                "source ordinary construction succeeds"
            );
            assert!(row["error"].is_null());
            let expected = match kind {
                Kind::FuturesExchange => "fetchHistoricalPositions",
                Kind::SpotExchange => "start",
                Kind::FuturesPublic => "mapTimeframe",
                _ => "connectAndSubscribe",
            };
            let message = String::from_utf16(&h.string(&err.message).unwrap()).unwrap();
            assert!(
                message.starts_with(&format!(
                    "Missing native member binding: {}.{expected} -> ",
                    class_name(kind)
                )),
                "paired native required-binding rejection"
            );
            continue;
        }
        match result {
            Err(err) => {
                error(&h, &err, &row["error"]);
            }
            Ok(first) => {
                assert!(row["error"].is_null());
                shape(&first, &row["snapshot"], own_floor);
                if flag(&h.cfg, "pair") {
                    let second = construct_class(&h, kind).unwrap();
                    for expected in row["pair"]["functions"].as_array().unwrap() {
                        let key = expected["key"].as_str().unwrap();
                        assert!(
                            !Rc::ptr_eq(&get(&first, key), &get(&second, key)),
                            "native fresh per-instance arrow"
                        );
                        assert_eq!(expected["fresh"], true);
                    }
                    for expected in row["pair"]["containers"].as_array().unwrap() {
                        let key = expected["key"].as_str().unwrap();
                        let shared = Rc::ptr_eq(&get(&first, key), &get(&second, key));
                        assert_eq!(
                            shared,
                            expected["shared"].as_bool().unwrap(),
                            "native Map/object isolation or imported identity"
                        );
                    }
                    assert_eq!(row["pair"]["prototypeOwn"], false);
                    assert!(
                        !matches!(&*get(&first, "mapTimeframe"), Raw::Function(_)),
                        "prototype method is not an own arrow"
                    );
                    h.assign(&first, &units("isDisposed"), h.boolean(true))
                        .unwrap();
                    let member = match kind {
                        Kind::FuturesExchange => "dispose",
                        Kind::SpotExchange => "tick",
                        _ => "onMessage",
                    };
                    let detached = get(&first, member);
                    let Raw::Function(f) = &*detached else {
                        panic!("native detachable arrow")
                    };
                    settle((f.call)(vec![get(&h.cfg, "argument")]).unwrap()).unwrap();
                }
                let invoke = get(&h.cfg, "invoke");
                if h.truthy(&invoke) {
                    let name = String::from_utf16(&h.string(&invoke).unwrap()).unwrap();
                    let result =
                        settle(h.call(first, &name, vec![get(&h.cfg, "argument")]).unwrap())
                            .unwrap();
                    native(
                        &result,
                        &row["invocation"],
                        "native composed production return",
                    );
                }
            }
        }
        native(
            &Raw::Array(h.events.borrow().clone()),
            &row["events"],
            "native class effects",
        );
    }
}
fn poll_maps(h: &Host) -> Rc<Raw> {
    h.object(vec![
        (
            units("active"),
            h.object(h.maps.borrow()[1].clone()).unwrap(),
        ),
        (
            units("timeouts"),
            h.object(h.maps.borrow()[0].clone()).unwrap(),
        ),
        (
            units("cache"),
            h.object(h.maps.borrow()[2].clone()).unwrap(),
        ),
    ])
    .unwrap()
}
#[test]
fn polling_native() {
    let rows = oracle()["polling"].as_array().unwrap();
    assert_eq!(rows.len(), 16, "independent polling floor");
    for (index, row) in rows.iter().enumerate() {
        let h = setup(&row["cfg"]);
        if flag(&h.cfg, "sharedFirstPages") {
            let mut pages = h.items(&get(&h.cfg, "pages")).unwrap();
            let first = h.items(&pages[0]).unwrap()[0].clone();
            pages[1] = h.array(vec![first]);
            h.assign(&h.cfg, &units("pages"), h.array(pages)).unwrap();
        }
        let options = get(&h.cfg, "options");
        let key = [
            h.string(&get(&options, "symbol")).unwrap(),
            units(":"),
            h.string(&get(&options, "interval")).unwrap(),
        ]
        .concat();
        let cached = get(&h.cfg, "cached");
        if h.truthy(&cached) {
            h.map_set(Slot::Candle, &key, cached).unwrap();
        }
        let cleanup_slot = Rc::new(RefCell::new(None::<ohlcv_polling::Cleanup<Host>>));
        let holder = cleanup_slot.clone();
        let cb_host = h.clone();
        let callback: ohlcv_polling::CandleCallback<Host> = Rc::new(move |candle| {
            let pages = cb_host.items(&get(&cb_host.cfg, "pages"))?;
            let identity = pages.iter().any(|p| {
                cb_host
                    .items(p)
                    .unwrap()
                    .iter()
                    .any(|v| Rc::ptr_eq(v, &candle))
            });
            cb_host.effect(
                "candle",
                vec![
                    ("candle", candle.clone()),
                    ("identity", cb_host.boolean(identity)),
                ],
            )?;
            if let Raw::Object(changes) = &*get(&cb_host.cfg, "mutateCandle") {
                for (k, v) in changes.borrow().iter() {
                    cb_host.assign(&candle, k, v.clone())?;
                }
            }
            if flag(&cb_host.cfg, "cleanupOnCallback") {
                holder.borrow().as_ref().unwrap()()?;
            }
            if flag(&cb_host.cfg, "callbackFail") {
                return Err(cb_host.original.clone());
            }
            Ok(())
        });
        let operation = (|| -> Result<(), E> {
            let mut cleanup =
                ohlcv_polling::listen_ohlcv(h.clone(), options.clone(), callback.clone())?;
            *cleanup_slot.borrow_mut() = Some(cleanup.clone());
            let prefix = h.events.borrow().len();
            assert_eq!(
                prefix,
                row["prefix"].as_u64().unwrap() as usize,
                "native unawaited synchronous prefix"
            );
            if let Raw::Object(changes) = &*get(&h.cfg, "mutateOptions") {
                for (k, v) in changes.borrow().iter() {
                    h.assign(&options, k, v.clone())?;
                }
            }
            if flag(&h.cfg, "cleanupBefore") {
                cleanup()?;
            }
            h.drive()?;
            if flag(&h.cfg, "replace") {
                let old = cleanup;
                cleanup = ohlcv_polling::listen_ohlcv(h.clone(), options, callback)?;
                *cleanup_slot.borrow_mut() = Some(cleanup.clone());
                if flag(&h.cfg, "oldCleanup") {
                    old()?;
                }
                h.drive()?;
            }
            let fires = get(&h.cfg, "fire");
            let fires = if h.undefined(&fires) {
                0
            } else {
                h.numeric(&fires)? as usize
            };
            for _ in 0..fires {
                let timer = {
                    let mut timers = h.timers.borrow_mut();
                    let (_, callback, canceled) = timers
                        .iter_mut()
                        .rev()
                        .find(|(_, _, c)| !*c)
                        .expect("finite requested timer exists");
                    *canceled = true;
                    callback.clone()
                };
                h.spawn(timer());
                h.drive()?;
            }
            for (_, cached) in h.maps.borrow()[2].iter() {
                let original_cached = get(&h.cfg, "cached");
                let from_page = h
                    .items(&get(&h.cfg, "pages"))
                    .unwrap()
                    .iter()
                    .any(|page| h.items(page).unwrap().iter().any(|v| Rc::ptr_eq(v, cached)));
                assert!(
                    from_page || Rc::ptr_eq(cached, &original_cached),
                    "native cache retains original candle allocation"
                );
            }
            native(
                &poll_maps(&h),
                &row["beforeCleanup"],
                "native polling state before cleanup",
            );
            cleanup()?;
            native(
                &poll_maps(&h),
                &row["afterCleanup"],
                "native polling state after cleanup",
            );
            Ok(())
        })();
        if let Err(err) = operation {
            error(&h, &err, &row["error"]);
        } else {
            assert!(row["error"].is_null());
        }
        native(
            &Raw::Array(h.events.borrow().clone()),
            &row["events"],
            &format!(
                "native polling effects[{index}] actual={:?} expected={:?}",
                to_value(&Raw::Array(h.events.borrow().clone())),
                row["events"]
            ),
        );
    }
}
#[test]
fn futures_exchange_class_native() {
    class_check(Kind::FuturesExchange, 4, 32, 24)
}
#[test]
fn spot_exchange_class_native() {
    class_check(Kind::SpotExchange, 4, 22, 17)
}
#[test]
fn futures_public_class_native() {
    class_check(Kind::FuturesPublic, 5, 15, 12)
}
#[test]
fn spot_public_class_native() {
    class_check(Kind::SpotPublic, 4, 15, 12)
}
#[test]
fn futures_private_class_native() {
    class_check(Kind::FuturesPrivate, 5, 18, 12)
}
#[test]
fn spot_private_class_native() {
    class_check(Kind::SpotPrivate, 5, 11, 8)
}
