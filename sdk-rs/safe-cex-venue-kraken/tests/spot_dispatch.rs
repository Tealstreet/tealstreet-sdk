use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_venue_kraken::spot_dispatch::{self, SpotDispatchHost};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    process::Command,
    rc::{Rc, Weak},
    sync::OnceLock,
};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/spot-dispatch-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(value["header"]["hashes"].as_object().unwrap().len(), 2);
        value
    })
}
fn flag(config: &Value, key: &str) -> bool {
    config[key].as_bool().unwrap_or(false)
}
enum Symbol {
    Text(Vec<u16>),
    Bool(bool),
    Number(f64),
    Null,
    Undefined,
    Array,
    Object,
}
impl Symbol {
    fn truthy(&self) -> bool {
        match self {
            Self::Text(text) => !text.is_empty(),
            Self::Bool(value) => *value,
            Self::Number(value) => *value != 0.0 && !value.is_nan(),
            Self::Null | Self::Undefined => false,
            Self::Array | Self::Object => true,
        }
    }
}
enum Item {
    Value {
        id: u32,
        symbol: Rc<Symbol>,
        marker: f64,
    },
    Null,
}
struct Message {
    items: RefCell<Vec<Rc<Item>>>,
    kind: RefCell<String>,
}
struct Host {
    owner: Weak<Host>,
    config: Value,
    message: Rc<Message>,
    events: RefCell<Vec<Event>>,
    error: RefCell<Option<ExchangeError>>,
    pointer: usize,
    appended: Cell<bool>,
}
struct Items {
    message: Rc<Message>,
    cursor: usize,
}
impl Iterator for Items {
    type Item = Result<Rc<Item>, ExchangeError>;
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.message.items.borrow().get(self.cursor)?.clone();
        self.cursor += 1;
        Some(Ok(value))
    }
}
enum Event {
    Data,
    Symbol(u32),
    Type,
    Dispatch(&'static str, u32, bool, bool, f64),
    Refresh,
}
impl Host {
    fn item(tag: &str, id: u32) -> Rc<Item> {
        let symbol = match tag {
            "text" => Symbol::Text(vec![38634, 47, 0xd800]),
            "empty" => Symbol::Text(vec![]),
            "false" => Symbol::Bool(false),
            "zero" => Symbol::Number(0.0),
            "negativeZero" => Symbol::Number(-0.0),
            "nan" => Symbol::Number(f64::NAN),
            "null" => Symbol::Null,
            "undefined" => Symbol::Undefined,
            "array" => Symbol::Array,
            "object" => Symbol::Object,
            _ => panic!("unknown independent symbol"),
        };
        Rc::new(Item::Value {
            id,
            symbol: Rc::new(symbol),
            marker: 19.5,
        })
    }
    fn new(config: Value) -> Rc<Self> {
        let tags = [
            "text",
            "empty",
            "false",
            "zero",
            "negativeZero",
            "nan",
            "null",
            "undefined",
            "array",
            "object",
        ];
        let mut items = if flag(&config, "empty") {
            vec![]
        } else {
            tags.iter()
                .enumerate()
                .map(|(i, tag)| Self::item(tag, i as u32 + 1))
                .collect()
        };
        if flag(&config, "nullItem") {
            items.push(Rc::new(Item::Null));
        }
        let message = Rc::new(Message {
            items: RefCell::new(items),
            kind: RefCell::new(config["type"].as_str().unwrap_or("snapshot").into()),
        });
        let mut error = ExchangeError::new("dispatch failure");
        error.name = "DispatchError".into();
        let pointer = (&*error.metadata as *const _) as usize;
        Rc::new_cyclic(|owner| Self {
            owner: owner.clone(),
            config,
            message,
            events: RefCell::new(vec![]),
            error: RefCell::new(Some(error)),
            pointer,
            appended: Cell::new(false),
        })
    }
    fn fail(&self, point: &str) -> Result<(), ExchangeError> {
        if self.config["fail"] == point {
            Err(self.error.borrow_mut().take().unwrap())
        } else {
            Ok(())
        }
    }
    fn dispatch(
        &self,
        method: &'static str,
        symbol: Rc<Symbol>,
        item: Rc<Item>,
    ) -> Result<(), ExchangeError> {
        let Item::Value {
            id,
            symbol: original,
            marker,
        } = item.as_ref()
        else {
            panic!("null item reached dispatch")
        };
        self.events.borrow_mut().push(Event::Dispatch(
            method,
            *id,
            self.message
                .items
                .borrow()
                .iter()
                .any(|original| Rc::ptr_eq(original, &item)),
            Rc::ptr_eq(original, &symbol),
            *marker,
        ));
        self.fail(method)?;
        if flag(&self.config, "flipType") {
            *self.message.kind.borrow_mut() = "update".into();
        }
        if flag(&self.config, "append") && !self.appended.replace(true) {
            self.message.items.borrow_mut().push(Self::item("text", 31));
        }
        Ok(())
    }
}
impl SpotDispatchHost for Host {
    type Message = Message;
    type Item = Item;
    type Symbol = Symbol;
    type Items = Items;
    fn array_data(&self, message: &Message) -> Result<Option<Items>, ExchangeError> {
        assert!(
            std::ptr::eq(message, self.message.as_ref()),
            "DISPATCH native original message"
        );
        self.events.borrow_mut().push(Event::Data);
        self.fail("data")?;
        Ok(if flag(&self.config, "notArray") {
            None
        } else {
            Some(Items {
                message: self.owner.upgrade().unwrap().message.clone(),
                cursor: 0,
            })
        })
    }
    fn symbol(&self, item: &Item) -> Result<Rc<Symbol>, ExchangeError> {
        match item {
            Item::Value { id, symbol, .. } => {
                self.events.borrow_mut().push(Event::Symbol(*id));
                self.fail("symbol")?;
                Ok(symbol.clone())
            }
            Item::Null => {
                let mut error =
                    ExchangeError::new("Cannot read properties of null (reading 'symbol')");
                error.name = "TypeError".into();
                Err(error)
            }
        }
    }
    fn symbol_truthy(&self, symbol: &Symbol) -> bool {
        symbol.truthy()
    }
    fn is_snapshot(&self, message: &Message) -> Result<bool, ExchangeError> {
        self.events.borrow_mut().push(Event::Type);
        self.fail("type")?;
        Ok(*message.kind.borrow() == "snapshot")
    }
    fn handle_snapshot(&self, symbol: Rc<Symbol>, item: Rc<Item>) -> Result<(), ExchangeError> {
        self.dispatch("snapshot", symbol, item)
    }
    fn handle_delta(&self, symbol: Rc<Symbol>, item: Rc<Item>) -> Result<(), ExchangeError> {
        self.dispatch("delta", symbol, item)
    }
    fn refresh_balance(&self) -> Result<(), ExchangeError> {
        self.events.borrow_mut().push(Event::Refresh);
        self.fail("refresh")
    }
}
fn run(kind: &str, body: fn(&Host, &Message) -> Result<(), ExchangeError>) {
    let cases = oracle()[kind].as_array().unwrap();
    assert_eq!(
        cases.len(),
        if kind == "book" { 14 } else { 6 },
        "DISPATCH independent cases"
    );
    let mut count = 0;
    let mut failures = 0;
    for case in cases {
        let host = Host::new(case["config"].clone());
        match body(&host, &host.message) {
            Ok(()) => {
                assert!(case["failure"].is_null());
                assert_eq!(
                    case["returned"], true,
                    "DISPATCH source completes without awaiting refresh"
                )
            }
            Err(error) => {
                failures += 1;
                assert_eq!(error.name, case["failure"]["name"]);
                assert_eq!(error.message, case["failure"]["message"]);
                if case["failure"]["identity"] == true {
                    assert_eq!(
                        (&*error.metadata as *const _) as usize,
                        host.pointer,
                        "DISPATCH native original error identity"
                    )
                } else {
                    assert_eq!(error.name, "TypeError")
                }
            }
        }
        let events = host.events.borrow();
        let expected = case["trace"].as_array().unwrap();
        assert_eq!(events.len(), expected.len(), "DISPATCH native event count");
        count += events.len();
        for (event, value) in events.iter().zip(expected) {
            match event {
                Event::Data => assert_eq!(value["kind"], "data"),
                Event::Symbol(id) => {
                    assert_eq!(value["kind"], "symbol");
                    assert_eq!(*id, value["id"].as_u64().unwrap() as u32)
                }
                Event::Type => assert_eq!(value["kind"], "type"),
                Event::Refresh => assert_eq!(value["kind"], "refresh"),
                Event::Dispatch(method, id, item, symbol, marker) => {
                    assert_eq!(value["kind"], "dispatch");
                    assert_eq!(*method, value["method"], "DISPATCH native method");
                    assert_eq!(*id, value["id"].as_u64().unwrap() as u32);
                    assert!(*item, "DISPATCH native item identity");
                    assert!(*symbol, "DISPATCH native symbol identity");
                    assert_eq!(value["itemIdentity"], true);
                    assert_eq!(value["symbolIdentity"], true);
                    assert_eq!(
                        format!("{:016x}", marker.to_bits()),
                        value["marker"]["number"],
                        "DISPATCH native item marker"
                    )
                }
            }
        }
    }
    assert_eq!(
        (count, failures),
        if kind == "book" { (141, 6) } else { (10, 2) },
        "DISPATCH independent event/failure floors"
    );
}
#[test]
fn handle_book_live_source() {
    run("book", spot_dispatch::handle_book)
}
#[test]
fn handle_balances_live_source() {
    run("balances", spot_dispatch::handle_balances)
}
