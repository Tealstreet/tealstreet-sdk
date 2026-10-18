use safe_cex_venue_bitget::cancellation::{self as body, *};
use serde_json::{Value as Json, json};
use std::{
    cell::{Cell, RefCell},
    future::Future,
    io::Write,
    process::{Command, Stdio},
    rc::Rc,
    task::{Context, Poll, Waker},
};
fn key(s: &str) -> CancelKey {
    s.encode_utf16().collect()
}
#[derive(Clone, Debug, PartialEq)]
enum V {
    Undefined,
    Null,
    String(CancelKey),
    Bool(bool),
    Number(i32),
    Object(Vec<u16>),
    Push(CancelKey),
}
fn val(s: &str) -> Rc<V> {
    Rc::new(V::String(key(s)))
}
fn wire(v: &V) -> Json {
    match v {
        V::Undefined => json!({"undefined":true}),
        V::Null => json!({"null":true}),
        V::String(s) => json!({"string":s}),
        V::Bool(b) => json!({"scalar":b}),
        V::Number(n) => json!({"scalar":n}),
        V::Object(u) => json!({"object":u}),
        V::Push(_) => panic!("internal handle"),
    }
}
#[derive(Clone, Debug)]
struct Info {
    order: Rc<V>,
    client: Rc<V>,
}
#[derive(Clone, Debug)]
struct O {
    status: Option<safe_cex_types::OrderStatus>,
    id: CancelKey,
    symbol: CancelKey,
    tag: usize,
    kind: &'static str,
    info: Rc<Info>,
}
#[derive(Clone, Debug)]
struct Case {
    op: &'static str,
    basket: &'static str,
    uta: bool,
    failure: &'static str,
    mutation: &'static str,
}
fn cases() -> Vec<Case> {
    let ops = [
        "classic-all",
        "partition",
        "normal",
        "uta-normal",
        "algo",
        "symbol",
        "inverse-all",
        "inverse-normal",
        "inverse-algo",
    ];
    let mut out = vec![];
    for op in ops {
        for basket in ["empty", "small", "bulk"] {
            for uta in [false, true] {
                out.push(Case {
                    op,
                    basket,
                    uta,
                    failure: "ok",
                    mutation: "none",
                });
            }
        }
    }
    for op in ops {
        out.push(Case {
            op,
            basket: "small",
            uta: false,
            failure: "reject",
            mutation: "none",
        });
        if op != "partition" {
            out.push(Case {
                op,
                basket: "small",
                uta: false,
                failure: "report",
                mutation: "none",
            });
        }
    }
    for failure in ["fetch-get", "fetch-reject"] {
        out.push(Case {
            op: "classic-all",
            basket: "small",
            uta: true,
            failure,
            mutation: "none",
        });
    }
    for op in ops {
        out.push(Case {
            op,
            basket: "unicode",
            uta: false,
            failure: "ok",
            mutation: "none",
        });
    }
    out.push(Case {
        op: "inverse-normal",
        basket: "prototype",
        uta: false,
        failure: "ok",
        mutation: "none",
    });
    out.push(Case {
        op: "normal",
        basket: "empty",
        uta: false,
        failure: "ok",
        mutation: "ensure",
    });
    out.push(Case {
        op: "algo",
        basket: "unicode",
        uta: false,
        failure: "ok",
        mutation: "post",
    });
    for mutation in ["predicate", "filter", "delete"] {
        out.push(Case {
            op: if mutation == "filter" {
                "symbol"
            } else {
                "partition"
            },
            basket: "small",
            uta: false,
            failure: "ok",
            mutation,
        });
    }
    assert_eq!(out.len(), 88, "CANCEL independent native floor");
    out
}
fn orders(c: &Case) -> Vec<Rc<O>> {
    let empty = || {
        Rc::new(Info {
            order: Rc::new(V::Undefined),
            client: Rc::new(V::Undefined),
        })
    };
    let make = |id: CancelKey, symbol: CancelKey, tag, kind, info| {
        Rc::new(O {
            status: None,
            id,
            symbol,
            tag,
            kind,
            info,
        })
    };
    match c.basket {
        "empty" => vec![],
        "small" => {
            let ids = ["n__x", "n", "v__sl", "a", "b", "c"];
            let symbols = ["10", "2", "alpha", "alpha", "2", "2"];
            let kinds = ["normal", "normal", "virtual", "algo", "normal", "algo"];
            let infos = [
                Rc::new(Info {
                    order: Rc::new(V::Undefined),
                    client: val("n"),
                }),
                Rc::new(Info {
                    order: val("server"),
                    client: Rc::new(V::Undefined),
                }),
                Rc::new(Info {
                    order: Rc::new(V::Null),
                    client: Rc::new(V::Bool(false)),
                }),
                Rc::new(Info {
                    order: Rc::new(V::Object(vec![65, 0xd800, 66, 0xdc00])),
                    client: val("cidA"),
                }),
                Rc::new(Info {
                    order: Rc::new(V::Number(0)),
                    client: Rc::new(V::Undefined),
                }),
                Rc::new(Info {
                    order: Rc::new(V::Bool(false)),
                    client: val(""),
                }),
            ];
            (0..6)
                .map(|i| make(key(ids[i]), key(symbols[i]), i, kinds[i], infos[i].clone()))
                .collect()
        }
        "bulk" => {
            let mut out: Vec<_> = (0..51)
                .map(|i| make(key(&i.to_string()), key("Z"), i, "normal", empty()))
                .collect();
            out.push(make(key("0__x"), key("Z"), 51, "normal", empty()));
            out
        }
        "unicode" | "prototype" => vec![make(
            vec![65, 0xd800, 66, 0xdc00, 95, 95, 120],
            if c.basket == "prototype" {
                key("__proto__")
            } else {
                vec![89, 0xdc00]
            },
            0,
            "normal",
            empty(),
        )],
        _ => unreachable!(),
    }
}
fn extra(c: &Case) -> Rc<O> {
    let source = orders(&Case {
        basket: "unicode",
        ..c.clone()
    });
    let mut o = (*source[0]).clone();
    if matches!(c.mutation, "predicate" | "filter") {
        o.tag = 1;
    }
    Rc::new(o)
}
#[derive(Clone)]
struct Request {
    endpoint: &'static str,
    fields: CancelPayload<Rc<V>>,
}
#[derive(Clone)]
enum Step {
    Post(Request, Vec<String>),
    Delegate(&'static str, Vec<Rc<O>>),
}
struct Model {
    steps: Vec<Step>,
    header: Vec<String>,
    write: bool,
    catch: bool,
    keep_going: bool,
    thrown: Option<&'static str>,
    trace: Vec<String>,
    post_count: usize,
    delegate_count: usize,
    error_count: usize,
    schedules: usize,
}
fn request(endpoint: &'static str, fields: CancelPayload<Rc<V>>, prefix: &[&str]) -> Step {
    Step::Post(
        Request { endpoint, fields },
        prefix.iter().map(|s| s.to_string()).collect(),
    )
}
fn value(s: &str) -> CancelArgument<Rc<V>> {
    CancelArgument::Value(val(s))
}
const ALL: &str = "/api/v2/mix/order/cancel-all-orders";
const NORMAL: &str = "/api/v2/mix/order/batch-cancel-orders";
const ALGO: &str = "/api/v2/mix/order/cancel-plan-order";
const UTA: &str = "/api/v3/trade/cancel-order";
fn model(c: &Case, os: &[Rc<O>]) -> Model {
    let mut m = Model {
        steps: vec![],
        header: vec![],
        write: false,
        catch: true,
        keep_going: false,
        thrown: None,
        trace: vec![],
        post_count: 0,
        delegate_count: 0,
        error_count: 0,
        schedules: 0,
    };
    let all = || os.to_vec();
    let delegate = |tag, items| Step::Delegate(tag, items);
    let raw = |i: usize| os[i].id.clone();
    let base = |i: usize| {
        let id = raw(i);
        let end = id
            .windows(2)
            .position(|v| v == [95, 95])
            .unwrap_or(id.len());
        id[..end].to_vec()
    };
    match c.op {
        "classic-all" => {
            m.header = vec!["ensure".into(), "uta".into()];
            if c.uta {
                m.header
                    .extend(["fetch-normal".into(), "fetch-strategy".into()]);
                m.steps.push(delegate(
                    "uta-normal",
                    os.first().cloned().into_iter().collect(),
                ));
                m.steps.push(delegate(
                    "uta-strategy",
                    if os.len() > 3 {
                        vec![os[3].clone()]
                    } else {
                        os.first().cloned().into_iter().collect()
                    },
                ));
            } else {
                m.steps.push(request(
                    ALL,
                    vec![
                        ("productType", value("product")),
                        ("marginCoin", value("margin")),
                    ],
                    &["product", "margin"],
                ));
                m.write = true;
            }
        }
        "partition" => {
            m.catch = false;
            for o in os {
                m.header.push(format!("virtual:{}", o.tag));
            }
            for o in os.iter().filter(|o| o.kind != "virtual") {
                m.header.push(format!("algo:{}", o.tag));
            }
            let normal = match c.basket {
                "small" => [0, 1, 4, 2].into_iter().map(|i| os[i].clone()).collect(),
                _ => all(),
            };
            let algos = if c.basket == "small" {
                vec![os[3].clone(), os[5].clone()]
            } else {
                vec![]
            };
            if !normal.is_empty() {
                m.steps.push(delegate("normal", normal));
            }
            if !algos.is_empty() {
                m.steps.push(delegate("algo", algos));
            }
        }
        "normal" if c.uta => {
            m.header = vec!["ensure".into(), "uta".into()];
            m.catch = false;
            m.steps.push(delegate("uta-normal", all()));
        }
        "algo" if c.uta => {
            m.header = vec!["ensure".into(), "uta".into()];
            m.catch = false;
            m.steps.push(delegate("uta-strategy", all()));
        }
        "normal" | "inverse-normal" => {
            let inverse = c.op == "inverse-normal";
            if inverse {
                m.keep_going = true;
            } else {
                m.header = vec!["ensure".into(), "uta".into()];
                m.write = !os.is_empty();
            }
            let groups: Vec<(CancelKey, Vec<usize>)> = match c.basket {
                "empty" => vec![],
                "small" => vec![
                    (key("2"), if inverse { vec![1, 4, 5] } else { vec![4, 5] }),
                    (key("10"), vec![0]),
                    (key("alpha"), vec![2, 3]),
                ],
                "bulk" => vec![(key("Z"), (0..51).collect())],
                "unicode" => vec![(vec![89, 0xdc00], vec![0])],
                "prototype" => {
                    m.thrown = Some("prototype");
                    vec![]
                }
                _ => unreachable!(),
            };
            for (symbol, indices) in groups {
                let margin = if inverse {
                    let mut coin = key("coin/");
                    coin.extend(&symbol);
                    CancelArgument::Value(Rc::new(V::String(coin)))
                } else {
                    value("margin")
                };
                for (lot_index, lot) in indices.chunks(50).enumerate() {
                    let mut prefix = vec![];
                    if inverse && lot_index == 0 {
                        prefix.push("coin");
                    }
                    prefix.extend(if inverse {
                        vec!["product"]
                    } else {
                        vec!["margin", "product"]
                    });
                    m.steps.push(request(
                        NORMAL,
                        vec![
                            ("symbol", CancelArgument::String(symbol.clone())),
                            (
                                "orderIdList",
                                CancelArgument::OrderIds(lot.iter().map(|i| base(*i)).collect()),
                            ),
                            ("marginCoin", margin.clone()),
                            ("productType", value("product")),
                        ],
                        &prefix,
                    ));
                }
            }
        }
        "uta-normal" => {
            m.write = !os.is_empty();
            let indices: Vec<usize> = match c.basket {
                "empty" => vec![],
                "small" => vec![0, 2, 3, 4, 5],
                "bulk" => (0..51).collect(),
                _ => vec![0],
            };
            for i in indices {
                let mut fields = vec![("category", value("category"))];
                if c.basket == "small" && i == 0 {
                    fields.push(("clientOid", value("n")));
                } else if c.basket == "small" && i == 3 {
                    fields.push(("orderId", CancelArgument::Value(os[3].info.order.clone())));
                    fields.push(("clientOid", value("cidA")));
                } else {
                    fields.push((
                        "orderId",
                        CancelArgument::Value(Rc::new(V::String(base(i)))),
                    ));
                }
                m.steps.push(request(UTA, fields, &["category"]));
            }
        }
        "algo" | "inverse-algo" => {
            let inverse = c.op == "inverse-algo";
            if inverse {
                m.keep_going = true;
            } else {
                m.header = vec!["ensure".into(), "uta".into()];
                m.write = !os.is_empty();
            }
            for o in os {
                let mut plan = key(if inverse { "inverse-plan/" } else { "plan/" });
                plan.extend(&o.id);
                let margin = if inverse {
                    let mut v = key("coin/");
                    v.extend(&o.symbol);
                    CancelArgument::Value(Rc::new(V::String(v)))
                } else {
                    value("margin")
                };
                let prefix = if inverse {
                    vec![
                        "coin".to_string(),
                        format!("inverse-plan:{}", o.tag),
                        "product".into(),
                    ]
                } else {
                    vec![format!("plan:{}", o.tag), "margin".into(), "product".into()]
                };
                m.steps.push(Step::Post(
                    Request {
                        endpoint: ALGO,
                        fields: vec![
                            ("orderId", CancelArgument::String(o.id.clone())),
                            ("symbol", CancelArgument::String(o.symbol.clone())),
                            ("marginCoin", margin),
                            ("productType", value("product")),
                            ("planType", CancelArgument::Value(Rc::new(V::String(plan)))),
                        ],
                    },
                    prefix,
                ));
            }
        }
        "symbol" => {
            let selection = if c.basket == "small" {
                if c.mutation == "filter" {
                    vec![os[4].clone(), os[5].clone()]
                } else {
                    vec![os[1].clone(), os[4].clone(), os[5].clone()]
                }
            } else {
                vec![]
            };
            m.steps.push(delegate("orders", selection));
        }
        "inverse-all" => {
            for coin in ["BTC", "ETH"] {
                m.steps.push(request(
                    ALL,
                    vec![
                        ("productType", value("product")),
                        ("marginCoin", CancelArgument::String(key(coin))),
                    ],
                    &["product"],
                ));
            }
        }
        _ => unreachable!(),
    }
    m.trace = m.header.clone();
    if c.failure.starts_with("fetch-") {
        if c.failure == "fetch-get" {
            m.trace.pop();
        }
        m.trace.push("error".into());
        m.error_count = 1;
        return m;
    }
    if m.thrown == Some("prototype") {
        return m;
    }
    let mut stopped = false;
    for (i, step) in m.steps.iter().enumerate() {
        match step {
            Step::Post(_, prefix) => {
                m.trace.extend(prefix.clone());
                m.trace.push("post".into());
                m.post_count += 1;
            }
            Step::Delegate(tag, _) => {
                m.trace.push(format!("delegate:{tag}"));
                m.delegate_count += 1;
            }
        }
        if i == 0 && c.failure != "ok" {
            if m.catch {
                m.trace.push("error".into());
                m.error_count += 1;
                if c.failure == "report" {
                    m.thrown = Some("report");
                    stopped = true;
                    break;
                }
                if !m.keep_going {
                    if !matches!(c.op, "classic-all" | "inverse-all") {
                        m.thrown = Some("primary");
                    }
                    stopped = true;
                    break;
                }
            } else {
                m.thrown = Some("primary");
                stopped = true;
                break;
            }
        }
    }
    if !stopped && m.write {
        m.trace.push("write".into());
        m.schedules = 1;
    }
    m
}
#[derive(Debug)]
struct E {
    kind: &'static str,
    message: CancelKey,
}
struct Host {
    c: Case,
    os: Vec<Rc<O>>,
    arg: CancelOrders<Rc<O>>,
    extra: Rc<O>,
    model: Model,
    trace: RefCell<Vec<String>>,
    requests: RefCell<Vec<Request>>,
    delegates: RefCell<Vec<(&'static str, Vec<Rc<O>>)>>,
    errors: RefCell<Vec<Rc<V>>>,
    schedules: Cell<usize>,
    primary_used: Cell<bool>,
    primary: Rc<E>,
    report: Rc<E>,
    prototype: Rc<E>,
    post_index: Cell<usize>,
    delegate_index: Cell<usize>,
}
impl Host {
    fn new(c: Case) -> Self {
        let os = orders(&c);
        let extra = extra(&c);
        let mut effective = os.clone();
        match c.mutation {
            "ensure" | "post" => effective.push(extra.clone()),
            "predicate" | "filter" => effective[1] = extra.clone(),
            _ => (),
        }
        let model_case = Case {
            basket: if c.mutation == "ensure" {
                "unicode"
            } else {
                c.basket
            },
            ..c.clone()
        };
        let mut model = model(&model_case, &effective);
        if c.mutation == "delete" {
            model.steps.clear();
            model.header = vec!["virtual:0".into()];
            model.trace = model.header.clone();
            model.thrown = Some("primary");
            model.post_count = 0;
            model.delegate_count = 0;
            model.error_count = 0;
            model.schedules = 0;
        }
        let arg = Rc::new(RefCell::new(os.clone()));
        Self {
            c,
            os,
            arg,
            extra,
            model,
            trace: RefCell::new(vec![]),
            requests: RefCell::new(vec![]),
            delegates: RefCell::new(vec![]),
            errors: RefCell::new(vec![]),
            schedules: Cell::new(0),
            primary_used: Cell::new(false),
            primary: Rc::new(E {
                kind: "primary",
                message: key("api failure"),
            }),
            report: Rc::new(E {
                kind: "report",
                message: key("report failure"),
            }),
            prototype: Rc::new(E {
                kind: "prototype",
                message: key("acc[key].push is not a function"),
            }),
            post_index: Cell::new(0),
            delegate_index: Cell::new(0),
        }
    }
    fn record(&self, e: impl Into<String>) {
        self.trace.borrow_mut().push(e.into());
    }
    fn result<'a, T: 'a>(&'a self, value: T) -> CancelFuture<'a, T, Rc<E>> {
        let fail = self.c.failure != "ok" && !self.primary_used.replace(true);
        let e = self.primary.clone();
        Box::pin(async move { if fail { Err(e) } else { Ok(value) } })
    }
    fn delegate(
        &self,
        tag: &'static str,
        os: Vec<Rc<O>>,
    ) -> Result<CancelFuture<'_, (), Rc<E>>, Rc<E>> {
        self.record(format!("delegate:{tag}"));
        let expected: Vec<_> = self
            .model
            .steps
            .iter()
            .filter_map(|s| {
                if let Step::Delegate(t, o) = s {
                    Some((*t, o))
                } else {
                    None
                }
            })
            .collect();
        let index = self.delegate_index.get();
        assert!(index < expected.len(), "CANCEL native unexpected delegate");
        assert_eq!(tag, expected[index].0, "CANCEL native delegate target");
        assert_eq!(
            os.len(),
            expected[index].1.len(),
            "CANCEL native independent delegate count"
        );
        for (a, b) in os.iter().zip(expected[index].1) {
            assert!(
                Rc::ptr_eq(a, b),
                "CANCEL native original order alias and ordering"
            );
        }
        self.delegate_index.set(index + 1);
        self.delegates.borrow_mut().push((tag, os));
        Ok(self.result(()))
    }
}
impl CancellationHost for Host {
    type Error = Rc<E>;
    type Order = Rc<O>;
    type Value = Rc<V>;
    fn undefined_order() -> Self::Order {
        Rc::new(O {
            status: None,
            id: vec![],
            symbol: vec![],
            tag: usize::MAX,
            kind: "undefined",
            info: Rc::new(Info {
                order: Rc::new(V::Undefined),
                client: Rc::new(V::Undefined),
            }),
        })
    }
    fn ensure_account_type(&self) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.record("ensure");
        Ok(Box::pin(async {
            if self.c.mutation == "ensure" {
                self.arg.borrow_mut().push(self.extra.clone());
            }
            Ok(())
        }))
    }
    fn uses_uta(&self) -> Result<bool, Self::Error> {
        self.record("uta");
        Ok(self.c.uta)
    }
    fn promise_all_function(&self) -> Result<Self::Value, Self::Error> {
        Ok(val("Promise.all"))
    }
    fn promise_all<'a>(
        &'a self,
        function: Self::Value,
        normal: CancelFuture<'a, CancelOrders<Self::Order>, Self::Error>,
        strategy: CancelFuture<'a, CancelOrders<Self::Order>, Self::Error>,
    ) -> Result<
        CancelFuture<'a, (CancelOrders<Self::Order>, CancelOrders<Self::Order>), Self::Error>,
        Self::Error,
    > {
        assert_eq!(
            *function,
            V::String(key("Promise.all")),
            "CANCEL native captured Promise.all"
        );
        // Fixture promises are already settled and remain owned by the host.
        Ok(Box::pin(async move {
            let a = normal.await?;
            let b = strategy.await?;
            Ok((a, b))
        }))
    }
    fn fetch_uta_normal(
        &self,
    ) -> Result<CancelFuture<'_, CancelOrders<Self::Order>, Self::Error>, Self::Error> {
        self.record("fetch-normal");
        if self.c.failure == "fetch-get" {
            return Err(self.primary.clone());
        }
        let data = self.os.first().cloned().into_iter().collect();
        let error = self.primary.clone();
        let reject = self.c.failure == "fetch-reject";
        Ok(Box::pin(async move {
            if reject {
                Err(error)
            } else {
                Ok(Rc::new(RefCell::new(data)))
            }
        }))
    }
    fn fetch_uta_strategy(
        &self,
    ) -> Result<CancelFuture<'_, CancelOrders<Self::Order>, Self::Error>, Self::Error> {
        self.record("fetch-strategy");
        let data = if self.os.len() > 3 {
            vec![self.os[3].clone()]
        } else {
            self.os.first().cloned().into_iter().collect()
        };
        Ok(Box::pin(async move { Ok(Rc::new(RefCell::new(data))) }))
    }
    fn cancel_normal(
        &self,
        o: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.delegate("normal", o.borrow().clone())
    }
    fn cancel_algo(
        &self,
        o: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.delegate("algo", o.borrow().clone())
    }
    fn cancel_uta_normal(
        &self,
        o: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.delegate("uta-normal", o.borrow().clone())
    }
    fn cancel_uta_strategy(
        &self,
        o: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.delegate("uta-strategy", o.borrow().clone())
    }
    fn cancel_orders(
        &self,
        o: CancelOrders<Self::Order>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        self.delegate("orders", o.borrow().clone())
    }
    fn virtual_tpsl(&self, o: &Self::Order) -> Result<bool, Self::Error> {
        if o.kind == "undefined" {
            return Err(self.primary.clone());
        }
        self.record(format!("virtual:{}", o.tag));
        if o.tag == 0 {
            if self.c.mutation == "predicate" {
                self.arg.borrow_mut()[1] = self.extra.clone();
            } else if self.c.mutation == "delete" {
                self.arg.borrow_mut().truncate(1);
            }
        }
        Ok(o.kind == "virtual")
    }
    fn algo(&self, o: &Self::Order) -> Result<bool, Self::Error> {
        self.record(format!("algo:{}", o.tag));
        Ok(o.kind == "algo")
    }
    fn spread(&self, o: &Self::Order) -> Result<Self::Order, Self::Error> {
        Ok(Rc::new((**o).clone()))
    }
    fn id(&self, o: &Self::Order) -> Result<CancelKey, Self::Error> {
        Ok(o.id.clone())
    }
    fn with_id(&self, mut o: Self::Order, id: CancelKey) -> Self::Order {
        Rc::get_mut(&mut o).expect("CANCEL fresh spread").id = id;
        o
    }
    fn is_terminal_order(&self, o: &Self::Order) -> Result<bool, Self::Error> {
        Ok(safe_cex_store::store::is_terminal_order_status(o.status))
    }
    fn symbol(&self, o: &Self::Order) -> Result<CancelKey, Self::Error> {
        if self.c.mutation == "filter" && o.tag == 0 {
            self.arg.borrow_mut()[1] = self.extra.clone();
        }
        Ok(o.symbol.clone())
    }
    fn info_order_id(&self, o: &Self::Order) -> Result<Self::Value, Self::Error> {
        assert!(
            Rc::ptr_eq(&o.info, &self.os[o.tag].info),
            "CANCEL native spread info alias"
        );
        Ok(o.info.order.clone())
    }
    fn info_client_oid(&self, o: &Self::Order) -> Result<Self::Value, Self::Error> {
        Ok(o.info.client.clone())
    }
    fn truthy(v: &Self::Value) -> bool {
        match &**v {
            V::Undefined | V::Null | V::Bool(false) => false,
            V::String(s) => !s.is_empty(),
            V::Number(n) => *n != 0,
            _ => true,
        }
    }
    fn is_undefined(v: &Self::Value) -> bool {
        matches!(**v, V::Undefined)
    }
    fn undefined() -> Self::Value {
        Rc::new(V::Undefined)
    }
    fn key_value(k: CancelKey) -> Self::Value {
        Rc::new(V::String(k))
    }
    fn equals_key(v: &Self::Value, k: &CancelKey) -> bool {
        matches!(&**v,V::String(s)if s==k)
    }
    fn product_type(&self) -> Result<Self::Value, Self::Error> {
        self.record("product");
        Ok(val("product"))
    }
    fn margin_coin(&self) -> Result<Self::Value, Self::Error> {
        self.record("margin");
        Ok(val("margin"))
    }
    fn uta_category(&self) -> Result<Self::Value, Self::Error> {
        self.record("category");
        Ok(val("category"))
    }
    fn plan_type(&self, o: &Self::Order) -> Result<Self::Value, Self::Error> {
        self.record(format!("plan:{}", o.tag));
        let mut k = key("plan/");
        k.extend(&o.id);
        Ok(Rc::new(V::String(k)))
    }
    fn margin_coin_for_symbol(&self, s: &CancelKey) -> Result<Self::Value, Self::Error> {
        self.record("coin");
        let mut k = key("coin/");
        k.extend(s);
        Ok(Rc::new(V::String(k)))
    }
    fn inverse_plan_type(&self, o: &Self::Order) -> Result<Self::Value, Self::Error> {
        self.record(format!("inverse-plan:{}", o.tag));
        let mut k = key("inverse-plan/");
        k.extend(&o.id);
        Ok(Rc::new(V::String(k)))
    }
    fn market_bases(&self) -> Result<Vec<CancelKey>, Self::Error> {
        Ok(["BTC", "ETH", "BTC"].map(key).to_vec())
    }
    fn store_orders(&self) -> Result<CancelOrders<Self::Order>, Self::Error> {
        Ok(self.arg.clone())
    }
    fn inverse_group_prepare(
        &self,
        g: &mut CancelGroups<Self::Order>,
        k: &CancelKey,
    ) -> Result<Self::Value, Self::Error> {
        if k == &key("__proto__") {
            return Ok(Rc::new(V::Undefined));
        }
        if !g.iter().any(|(x, _)| x == k) {
            g.push((k.clone(), vec![]));
        }
        Ok(Rc::new(V::Push(k.clone())))
    }
    fn inverse_group_push(
        &self,
        g: &mut CancelGroups<Self::Order>,
        k: CancelKey,
        o: Self::Order,
        push: Self::Value,
    ) -> Result<(), Self::Error> {
        if matches!(*push, V::Undefined) {
            return Err(self.prototype.clone());
        }
        assert_eq!(
            *push,
            V::Push(k.clone()),
            "CANCEL native captured group push"
        );
        g.iter_mut().find(|(x, _)| x == &k).unwrap().1.push(o);
        Ok(())
    }
    fn post(
        &self,
        endpoint: &'static str,
        fields: CancelPayload<Self::Value>,
    ) -> Result<CancelFuture<'_, (), Self::Error>, Self::Error> {
        let expected: Vec<_> = self
            .model
            .steps
            .iter()
            .filter_map(|s| {
                if let Step::Post(r, _) = s {
                    Some(r)
                } else {
                    None
                }
            })
            .collect();
        let i = self.post_index.get();
        assert!(i < expected.len(), "CANCEL native unexpected request");
        let r = expected[i];
        assert_eq!(endpoint, r.endpoint, "CANCEL native endpoint");
        assert_eq!(
            fields.len(),
            r.fields.len(),
            "CANCEL native own payload field count"
        );
        for ((name, a), (key, b)) in fields.iter().zip(&r.fields) {
            assert_eq!(name, key, "CANCEL native payload property order");
            match (a, b) {
                (CancelArgument::Value(a), CancelArgument::Value(b)) => {
                    assert_eq!(**a, **b, "CANCEL native scalar/UTF16 payload");
                    if matches!(**a, V::Object(_)) {
                        assert!(Rc::ptr_eq(a, b), "CANCEL native opaque info identity");
                    }
                }
                (CancelArgument::String(a), CancelArgument::String(b)) => {
                    assert_eq!(a, b, "CANCEL native symbol/order UTF16")
                }
                (CancelArgument::OrderIds(a), CancelArgument::OrderIds(b)) => {
                    assert_eq!(a, b, "CANCEL native decoded batch IDs")
                }
                _ => panic!("CANCEL native payload argument kind"),
            }
        }
        self.post_index.set(i + 1);
        self.record("post");
        self.requests
            .borrow_mut()
            .push(Request { endpoint, fields });
        Ok(Box::pin(async {
            self.result(()).await?;
            if self.c.mutation == "post" && self.post_index.get() == 1 {
                self.arg.borrow_mut().push(self.extra.clone());
            }
            Ok(())
        }))
    }
    fn schedule_write_check(&self) -> Result<(), Self::Error> {
        self.record("write");
        self.schedules.set(self.schedules.get() + 1);
        Ok(())
    }
    fn error_message(&self, e: &Self::Error) -> Result<Self::Value, Self::Error> {
        Ok(Rc::new(V::String(e.message.clone())))
    }
    fn emit_error(&self, m: Self::Value) -> Result<(), Self::Error> {
        assert_eq!(
            *m,
            V::String(key("api failure")),
            "CANCEL native decoded error message"
        );
        self.record("error");
        self.errors.borrow_mut().push(m);
        if self.c.failure == "report" {
            Err(self.report.clone())
        } else {
            Ok(())
        }
    }
}
fn block<F: Future>(f: F) -> F::Output {
    let w = Waker::noop().clone();
    let mut cx = Context::from_waker(&w);
    let mut f = std::pin::pin!(f);
    for _ in 0..16 {
        if let Poll::Ready(v) = f.as_mut().poll(&mut cx) {
            return v;
        }
    }
    panic!("CANCEL bounded native future floor exceeded")
}
fn run(c: Case) -> Json {
    let h = Host::new(c);
    let initial: Vec<_> = h.os.iter().map(|o| o.id.clone()).collect();
    let result = block(async {
        match h.c.op {
            "classic-all" => body::cancel_all_orders(&h).await,
            "partition" => body::cancel_orders(&h, h.arg.clone()).await,
            "normal" => body::cancel_normal_orders(&h, h.arg.clone()).await,
            "uta-normal" => body::cancel_uta_normal_orders(&h, h.arg.clone()).await,
            "algo" => body::cancel_algo_orders(&h, h.arg.clone()).await,
            "symbol" => body::cancel_symbol_orders(&h, key("2")).await,
            "inverse-all" => body::inverse_cancel_all_orders(&h).await,
            "inverse-normal" => body::inverse_cancel_normal_orders(&h, h.arg.clone()).await,
            "inverse-algo" => body::inverse_cancel_algo_orders(&h, h.arg.clone()).await,
            _ => unreachable!(),
        }
    });
    assert_eq!(
        result.as_ref().err().map(|e| e.kind),
        h.model.thrown,
        "CANCEL native error disposition: {:?}",
        h.c
    );
    if let Err(e) = &result {
        let expected = match e.kind {
            "primary" => &h.primary,
            "report" => &h.report,
            _ => &h.prototype,
        };
        assert!(
            Rc::ptr_eq(e, expected),
            "CANCEL native original error identity"
        );
        assert_eq!(e.message, expected.message, "CANCEL native error UTF16");
    }
    assert_eq!(
        &*h.trace.borrow(),
        &h.model.trace,
        "CANCEL native operation order: {:?}",
        h.c
    );
    assert_eq!(
        h.requests.borrow().len(),
        h.model.post_count,
        "CANCEL native independent request count"
    );
    assert_eq!(
        h.delegates.borrow().len(),
        h.model.delegate_count,
        "CANCEL native independent delegation count"
    );
    assert_eq!(
        h.errors.borrow().len(),
        h.model.error_count,
        "CANCEL native independent error count"
    );
    assert_eq!(
        h.schedules.get(),
        h.model.schedules,
        "CANCEL native write-check count"
    );
    assert_eq!(
        h.os.iter().map(|o| o.id.clone()).collect::<Vec<_>>(),
        initial,
        "CANCEL native caller order records unchanged"
    );
    assert_eq!(
        h.arg.borrow().len(),
        if h.c.mutation == "delete" {
            1
        } else {
            h.os.len() + usize::from(matches!(h.c.mutation, "ensure" | "post"))
        },
        "CANCEL native live caller array length"
    );
    // Every native value and count is asserted before projection or source comparison.
    let requests:Vec<_>=h.requests.borrow().iter().map(|r|json!({"endpoint":r.endpoint,"fields":r.fields.iter().map(|(name,v)|json!([name,match v{CancelArgument::Value(v)=>wire(v),CancelArgument::String(k)=>json!({"string":k}),CancelArgument::OrderIds(ids)=>json!({"ids":ids})}])).collect::<Vec<_>>()})).collect();
    let delegates:Vec<_>=h.delegates.borrow().iter().map(|(tag,os)|json!({"tag":tag,"ids":os.iter().map(|o|o.id.clone()).collect::<Vec<_>>(),"aliases":vec![true;os.len()]})).collect();
    let thrown = match result {
        Ok(()) => Json::Null,
        Err(e) if e.kind == "prototype" => json!({"name":"TypeError","message":e.message}),
        Err(e) => json!(e.kind),
    };
    json!({"trace":*h.trace.borrow(),"requests":requests,"delegates":delegates,"errors":h.errors.borrow().iter().map(|v|wire(v)).collect::<Vec<_>>(),"schedules":h.schedules.get(),"thrown":thrown})
}
fn input(c: &Case) -> Json {
    let os = orders(c);
    let o = extra(c);
    let extra_wire = json!({"id":o.id,"symbol":o.symbol,"tag":o.tag,"kind":o.kind,"orderId":wire(&o.info.order),"clientOid":wire(&o.info.client)});
    json!({"op":c.op,"uta":c.uta,"mutation":c.mutation,"extra":extra_wire,"failure":c.failure,"symbol":key("2"),"coins":[key("BTC"),key("ETH"),key("BTC")],"orders":os.iter().map(|o|json!({"id":o.id,"symbol":o.symbol,"tag":o.tag,"kind":o.kind,"orderId":wire(&o.info.order),"clientOid":wire(&o.info.client)})).collect::<Vec<_>>()})
}
#[test]
fn native_cancellation() {
    for c in cases() {
        run(c);
    }
}
#[test]
#[ignore = "requires local TypeScript oracle"]
fn source_cancellation() {
    let cs = cases();
    let native: Vec<_> = cs.iter().cloned().map(run).collect();
    let inputs: Vec<_> = cs.iter().map(input).collect();
    let mut child = Command::new("node")
        .arg(format!(
            "{}/scripts/cancellation-capture.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(serde_json::to_string(&inputs).unwrap().as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Vec<Json> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(source.len(), 88, "CANCEL independent source floor");
    for (i, (a, b)) in native.iter().zip(source.iter()).enumerate() {
        assert_eq!(a, b, "CANCEL source case {i}: {:?}", cs[i]);
    }
}
