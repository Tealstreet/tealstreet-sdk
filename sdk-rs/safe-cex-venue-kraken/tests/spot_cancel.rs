use safe_cex_exchange::{
    js_runtime::{
        promise::JsFuture,
        value::{Js, Thrown},
    },
    runtime::ExchangeError,
};
use safe_cex_venue_kraken::spot_cancel::{self, SpotCancelHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
    task::{Context, Poll, Waker},
};
fn oracle() -> &'static Value {
    static DATA: OnceLock<Value> = OnceLock::new();
    DATA.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/spot-cancel-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 1);
        value
    })
}
fn flag(value: &Value, key: &str) -> bool {
    value[key].as_bool().unwrap_or(false)
}
fn text(value: &Value) -> Vec<u16> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as u16)
        .collect()
}
struct Order {
    id: RefCell<Rc<Vec<u16>>>,
    index: u32,
}
struct Orders(RefCell<Vec<Rc<Order>>>);
struct Items {
    orders: Rc<Orders>,
    cursor: usize,
}
impl Iterator for Items {
    type Item = Result<Rc<Order>, Thrown>;
    fn next(&mut self) -> Option<Self::Item> {
        let order = self.orders.0.borrow().get(self.cursor)?.clone();
        self.cursor += 1;
        Some(Ok(order))
    }
}
enum Event {
    Id(u32),
    Post(&'static str, &'static str, Vec<u16>, bool),
    Error(String, &'static str),
}
struct Host {
    owner: Weak<Host>,
    config: Value,
    orders: Rc<Orders>,
    posts: Cell<usize>,
    released: Cell<usize>,
    events: RefCell<Vec<Event>>,
    error: Thrown,
    pointer: usize,
}
impl Host {
    fn new(config: Value) -> Rc<Self> {
        let orders = Rc::new(Orders(RefCell::new(
            (0..config["count"].as_u64().unwrap() as usize)
                .map(|i| {
                    Self::order(
                        if i == 0 {
                            vec![38634, 47, 0xd800]
                        } else {
                            format!("id-{i}").encode_utf16().collect()
                        },
                        i as u32 + 1,
                    )
                })
                .collect(),
        )));
        let mut error = ExchangeError::new(if flag(&config, "emptyMessage") {
            ""
        } else {
            "cancel failure"
        });
        error.name = "CancelError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        Rc::new_cyclic(|owner| Self {
            owner: owner.clone(),
            config,
            orders,
            posts: Cell::new(0),
            released: Cell::new(0),
            events: RefCell::new(vec![]),
            error: error.into(),
            pointer,
        })
    }
    fn order(id: Vec<u16>, index: u32) -> Rc<Order> {
        Rc::new(Order {
            id: RefCell::new(Rc::new(id)),
            index,
        })
    }
    fn fail(&self, point: &str) -> Result<(), Thrown> {
        if self.config["fail"] == point {
            Err(self.error.clone())
        } else {
            Ok(())
        }
    }
}
impl SpotCancelHost for Host {
    type Orders = Orders;
    type Order = Order;
    type Id = Vec<u16>;
    type Items = Items;
    fn length(&self, orders: &Orders) -> Result<usize, Thrown> {
        Ok(orders.0.borrow().len())
    }
    fn first(&self, orders: &Orders) -> Result<Rc<Order>, Thrown> {
        Ok(orders.0.borrow()[0].clone())
    }
    fn items(&self, orders: Rc<Orders>) -> Result<Items, Thrown> {
        Ok(Items { orders, cursor: 0 })
    }
    fn id(&self, order: &Order) -> Result<Rc<Vec<u16>>, Thrown> {
        self.events.borrow_mut().push(Event::Id(order.index));
        self.fail(&format!("id-{}", order.index))?;
        Ok(order.id.borrow().clone())
    }
    fn post_cancel(
        &self,
        endpoint: &'static str,
        field: &'static str,
        id: Rc<Vec<u16>>,
    ) -> Result<JsFuture<'static>, Thrown> {
        let post = self.posts.get() + 1;
        self.posts.set(post);
        let identity = self
            .orders
            .0
            .borrow()
            .iter()
            .any(|order| Rc::ptr_eq(&order.id.borrow(), &id));
        self.events
            .borrow_mut()
            .push(Event::Post(endpoint, field, (*id).clone(), identity));
        self.fail(&format!("sync-{post}"))?;
        let host = self.owner.upgrade().unwrap();
        Ok(Box::pin(poll_fn(move |_| {
            if host.released.get() < post {
                Poll::Pending
            } else {
                Poll::Ready(host.fail(&format!("async-{post}")).map(|()| Js::Undefined))
            }
        })))
    }
    fn error_message(&self, error: &Thrown, fallback: &'static str) -> Result<Js, Thrown> {
        assert_eq!(
            (&*error.native.as_ref().unwrap().metadata as *const _) as usize,
            self.pointer,
            "CANCEL native caught original error identity"
        );
        let message = error.value.get("message")?;
        Ok(if message.truthy() {
            message
        } else {
            Js::str(fallback)
        })
    }
    fn emit_error(&self, message: &Js, context: &'static str) -> Result<(), Thrown> {
        self.events
            .borrow_mut()
            .push(Event::Error(message.to_display(), context));
        Ok(())
    }
}
#[test]
fn cancel_orders_live_source() {
    let cases = oracle()["rows"].as_array().unwrap();
    assert_eq!(cases.len(), 14, "CANCEL independent cases");
    let mut total = 0;
    let mut failures = 0;
    let mut checkpoint_count = 0;
    for case in cases {
        let host = Host::new(case["config"].clone());
        let mut future = Box::pin(spot_cancel::cancel_orders(
            host.as_ref(),
            host.orders.clone(),
        ));
        let mut result = None;
        let mut cx = Context::from_waker(Waker::noop());
        for step in 0..5 {
            if step > 0 && host.posts.get() > host.released.get() {
                if step == 1 {
                    if flag(&host.config, "append") {
                        host.orders
                            .0
                            .borrow_mut()
                            .push(Host::order("added".encode_utf16().collect(), 31));
                    }
                    if flag(&host.config, "remove") {
                        host.orders.0.borrow_mut().remove(1);
                    }
                    if flag(&host.config, "changeId") {
                        *host.orders.0.borrow()[1].id.borrow_mut() =
                            Rc::new("changed".encode_utf16().collect());
                    }
                }
                host.released.set(host.released.get() + 1)
            }
            if result.is_none() {
                if let Poll::Ready(value) = future.as_mut().poll(&mut cx) {
                    result = Some(value)
                }
            }
            let expected = &case["checkpoints"][step];
            assert_eq!(
                host.events.borrow().len(),
                expected["events"].as_u64().unwrap() as usize,
                "CANCEL native checkpoint events"
            );
            assert_eq!(
                result.is_some(),
                expected["done"].as_bool().unwrap(),
                "CANCEL native sequential settlement"
            );
            checkpoint_count += 1;
        }
        match result.expect("cancel settled") {
            Ok(()) => assert!(case["failure"].is_null()),
            Err(error) => {
                let error = error.to_exchange_error();
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                assert_ne!(
                    (&*error.metadata as *const _) as usize,
                    host.pointer,
                    "CANCEL native fresh error identity"
                );
                assert_eq!(case["failure"]["identity"], false)
            }
        }
        let events = host.events.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(events.len(), expected.len());
        total += events.len();
        for (event, value) in events.iter().zip(expected) {
            match event {
                Event::Id(index) => {
                    assert_eq!(value["kind"], "id");
                    assert_eq!(*index, value["index"].as_u64().unwrap() as u32)
                }
                Event::Post(endpoint, field, id, identity) => {
                    assert_eq!(value["kind"], "post");
                    assert_eq!(*endpoint, value["endpoint"], "CANCEL native endpoint");
                    assert_eq!(*field, value["field"], "CANCEL native ID field");
                    assert_eq!(*id, text(&value["id"]), "CANCEL native current original ID");
                    assert!(*identity);
                    assert_eq!(value["identity"], true)
                }
                Event::Error(message, context) => {
                    assert_eq!(value["kind"], "error");
                    assert_eq!(message, value["message"].as_str().unwrap());
                    assert_eq!(*context, value["context"])
                }
            }
        }
    }
    assert_eq!(
        (total, checkpoint_count, failures),
        (49, 70, 7),
        "CANCEL independent event/checkpoint/failure floors"
    );
}
