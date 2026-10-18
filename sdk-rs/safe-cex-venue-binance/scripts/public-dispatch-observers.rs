use crate::{
    private_event_value::{EventOps, EventValue, number},
    public_dispatch::{CallbackList, Handler, Handlers},
    public_listeners::Parser,
    public_ticker_events::Failure,
    ticker_capture::{Capture, Dictionary, Target},
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_types::{Ticker, retained_json::RetainedJsonInput as Input};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

fn bits(value: f64) -> String {
    format!("{:016x}", value.to_bits())
}

fn retained(value: &Input) -> Value {
    match value {
        Input::Undefined => json!(["undefined"]),
        Input::Null => json!(["null"]),
        Input::Bool(value) => json!(["bool", value]),
        Input::Text(value) => json!(["text", value]),
        Input::Lexeme(value) => json!(["lexeme", value]),
        Input::Float(value) => json!(["number", bits(*value)]),
        Input::Array(values) => json!([
            "array",
            values.len(),
            values.iter().map(retained).collect::<Vec<_>>()
        ]),
        Input::Object(values) => json!([
            "object",
            values.len(),
            values
                .iter()
                .map(|(key, value)| json!([key, retained(value)]))
                .collect::<Vec<_>>()
        ]),
    }
}

fn ticker(row: &Ticker) -> Value {
    json!({
        "id":row.id,"symbol":row.symbol,"bid":bits(row.bid),"ask":bits(row.ask),
        "last":bits(row.last),"mark":bits(row.mark),"index":bits(row.index),
        "percentage":bits(row.percentage),"volume":bits(row.volume),
        "quoteVolume":bits(row.quote_volume),"openInterest":bits(row.open_interest),
        "fundingRate":bits(row.funding_rate),"fundingTime":bits(row.funding_time),
        "midPrice":row.mid_price.map(bits),"timestamp":row.timestamp.map(bits),
        "infoOwn":row.info.is_some(),"info":row.info.as_ref().map(retained),
    })
}

pub fn lookup(input: &Value, resolve: fn(&Dictionary, &str) -> Option<Target>) -> Value {
    let store = Rc::new(DefaultStore::new());
    let mut row: Ticker = serde_json::from_value(input["ticker"].clone()).unwrap();
    row.info = Some(Input::Object(vec![
        ("nan".encode_utf16().collect(), Input::Float(f64::NAN)),
        ("text".encode_utf16().collect(), Input::Text(vec![0xd800])),
        ("missing".encode_utf16().collect(), Input::Undefined),
        (
            "array".encode_utf16().collect(),
            Input::Array(vec![Input::Float(f64::INFINITY), Input::Float(-0.0)]),
        ),
        (vec![0xd801], Input::Null),
    ]));
    Store::update(
        store.as_ref(),
        StoreUpdate {
            tickers: Some(vec![row]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let typed_store: Rc<dyn Store> = store;
    let capture = Capture::new(typed_store.clone());
    let dictionary = capture.dictionary();
    let keys = dictionary.borrow().keys().cloned().collect::<Vec<_>>();
    let key = input["key"].as_str().unwrap();
    match resolve(&dictionary, key) {
        Some(Target::Row(row)) => json!({
            "kind":"row","keys":keys,"sameRow":Rc::ptr_eq(&row,dictionary.borrow().get(key).unwrap()),
            "belongsToStore":capture.belongs_to(&typed_store),"ticker":ticker(&row.borrow()),
        }),
        Some(target @ Target::StockInherited) => {
            json!({"kind":"inherited","keys":keys,"id":target.id()})
        }
        None => json!({"kind":"missing","keys":keys}),
    }
}

fn inert() -> Parser {
    Rc::new(|_| panic!("looking up a handler must not execute its callback"))
}

pub fn handler(input: &Value, get: fn(&Handlers, &str) -> Handler) -> Value {
    let handlers = Handlers::default();
    let own = Rc::new(RefCell::new(vec![inert(), inert()]));
    let prototype = Rc::new(RefCell::new(vec![inert()]));
    handlers
        .own
        .borrow_mut()
        .insert("event.BTC".into(), Handler::List(own.clone()));
    handlers
        .own
        .borrow_mut()
        .insert("flag".into(), Handler::Value(EventValue::Bool(false)));
    handlers
        .prototype
        .borrow_mut()
        .insert("event.ETH".into(), Handler::List(prototype.clone()));
    handlers
        .prototype
        .borrow_mut()
        .insert("flag".into(), Handler::List(prototype.clone()));
    match get(&handlers, input["key"].as_str().unwrap()) {
        Handler::List(list) => json!({
            "kind":"list","length":list.borrow().len(),"sameOwn":Rc::ptr_eq(&list,&own),
            "samePrototype":Rc::ptr_eq(&list,&prototype),"keys":handlers.keys(),
        }),
        Handler::CoreList(_, _) => panic!("no core handler was registered in this lookup case"),
        Handler::Value(value) => json!({
            "kind":value.type_of(),"callable":value.is_callable(),"truthy":value.truthy(),
            "length":if value.is_callable() {match value.get("length").unwrap() {EventValue::Number(v)=>Some(bits(v)),_=>None}}else{None},
            "keys":handlers.keys(),
        }),
    }
}

fn observe_packet(events: &EventValue, packets: &RefCell<Vec<EventValue>>) -> (bool, Value) {
    let first = events.read("0").unwrap();
    let mut saved = packets.borrow_mut();
    let same = saved
        .first()
        .is_none_or(|prior| prior.strict_equals(events));
    saved.push(events.clone());
    let mutated = match first.read("mutated").unwrap() {
        EventValue::Number(value) => Some(bits(value)),
        _ => None,
    };
    (
        same,
        json!({"symbol":first.read("s").unwrap().text().unwrap(),"mutated":mutated}),
    )
}

pub fn dispatch(
    input: &Value,
    run: impl FnOnce(
        &Handlers,
        &str,
        &mut dyn FnMut(&str) -> Result<(), Failure>,
    ) -> Result<(), Failure>,
) -> Value {
    let handlers = Handlers::default();
    let trace = Rc::new(RefCell::new(Vec::<String>::new()));
    let packets = Rc::new(RefCell::new(Vec::<EventValue>::new()));
    let observations = Rc::new(RefCell::new(Vec::<Value>::new()));
    let error = Rc::new(ExchangeError::new("configured failure"));
    let broadcast: CallbackList = Default::default();
    for (flag, key, label, prototype) in [
        ("broadcast", "event", "broadcast", false),
        ("exact", "event.BTC", "exact", false),
        ("prefix", "event.1m.ETH", "prefix", false),
        ("prefix", "event.2m.ADA", "inherited-prefix", true),
    ] {
        if input[flag] != true {
            continue;
        }
        let list = if flag == "broadcast" {
            broadcast.clone()
        } else {
            CallbackList::default()
        };
        let weak_list = Rc::downgrade(&list);
        let trace = trace.clone();
        let packets = packets.clone();
        let observations = observations.clone();
        let error = error.clone();
        let append = flag == "broadcast" && input["append"] == true;
        let mutate = flag == "broadcast" && input["mutate"] == true;
        let fail = flag == "broadcast" && input["handlerError"] == true;
        list.borrow_mut().push(Rc::new(move |events: EventValue| {
            trace.borrow_mut().push(label.into());
            if mutate {
                events.read("0")?.write("mutated", number(-0.0));
            }
            let (same, fields) = observe_packet(&events, &packets);
            observations
                .borrow_mut()
                .push(json!({"samePacket":same,"fields":fields}));
            if append {
                let trace = trace.clone();
                let packets = packets.clone();
                let observations = observations.clone();
                weak_list
                    .upgrade()
                    .unwrap()
                    .borrow_mut()
                    .push(Rc::new(move |events| {
                        trace.borrow_mut().push("appended".into());
                        let (same, fields) = observe_packet(&events, &packets);
                        observations
                            .borrow_mut()
                            .push(json!({"samePacket":same,"fields":fields}));
                        Ok(())
                    }));
            }
            if fail { Err(error.clone()) } else { Ok(()) }
        }));
        if prototype {
            handlers
                .prototype
                .borrow_mut()
                .insert(key.into(), Handler::List(list));
        } else {
            handlers
                .own
                .borrow_mut()
                .insert(key.into(), Handler::List(list));
        }
    }
    let result = run(&handlers, input["wire"].as_str().unwrap(), &mut |topic| {
        trace.borrow_mut().push(format!("stats:{topic}"));
        if input["clearOnStats"] == true {
            broadcast.borrow_mut().clear();
        }
        if input["statsError"] == true {
            Err(error.clone())
        } else {
            Ok(())
        }
    });
    json!({
        "trace":*trace.borrow(),"packets":*observations.borrow(),"broadcastLength":broadcast.borrow().len(),
        "error":result.err().map(|returned|json!({"name":returned.name,"message":returned.message,"sameError":Rc::ptr_eq(&returned,&error)})),
    })
}
