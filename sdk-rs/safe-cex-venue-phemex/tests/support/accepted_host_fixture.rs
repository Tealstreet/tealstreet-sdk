// Native accepted-contract fixture. Only implemented operations may be called;
// every unrelated host capability panics. Language operations use the shared Js
// carrier; mapped-key probes intentionally cover exact own keys only.
use safe_cex_exchange::js_runtime::value::{Js, Thrown, from_json};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    future::{Future, Ready},
    pin::Pin,
    rc::Rc,
};
#[derive(Clone)]
pub struct Host {
    pub trace: Rc<RefCell<Vec<Value>>>,
    pub market: Js,
    pub sentinel: Js,
    pub sent: Rc<RefCell<Vec<Js>>>,
    pub fail_send: bool,
}
impl Host {
    pub fn new(input: &Value) -> Self {
        Self {
            trace: Rc::new(RefCell::new(vec![])),
            sent: Rc::new(RefCell::new(vec![])),
            fail_send: input["failSend"] == true,
            market: input.get("market").map(from_json).unwrap_or(Js::Undefined),
            sentinel: Js::object(vec![
                ("name", Js::str("Sentinel")),
                ("message", Js::str("host refusal")),
            ]),
        }
    }
    fn get_value(&self, value: &Js, key: &[u16]) -> Result<Js, Thrown> {
        self.trace.borrow_mut().push(json!({"get":key}));
        if key == "explode".encode_utf16().collect::<Vec<_>>() {
            return Err(Thrown::new(self.sentinel.clone()));
        }
        value.get_key(key)
    }
    fn optional_value(&self, value: &Js, key: &[u16]) -> Result<Js, Thrown> {
        if value.is_nullish() {
            self.trace.borrow_mut().push(json!({"optionalNullish":key}));
            Ok(Js::Undefined)
        } else {
            self.get_value(value, key)
        }
    }
}
pub fn capture(value: &Js) -> Value {
    match value {
        Js::Undefined => json!({"kind":"undefined"}),
        Js::Null => json!({"kind":"null"}),
        Js::Bool(b) => json!({"kind":"boolean","value":b}),
        Js::Number(n) => {
            json!({"kind":"number","bits":if n.is_nan(){"NaN".to_owned()}else{format!("{:016x}",n.to_bits())}})
        }
        Js::String(s) => json!({"kind":"string","units":s.as_ref()}),
        Js::Array(_) => {
            json!({"kind":"array","items":value.array_values().expect("fixture array property reads").unwrap().iter().map(capture).collect::<Vec<_>>()})
        }
        Js::Object(_) => {
            let mut fields = serde_json::Map::new();
            for (key, value) in value.own_entries().expect("fixture own fields") {
                fields.insert(
                    String::from_utf16(&key).expect("fixture scalar field key"),
                    capture(&value),
                );
            }
            json!({"kind":"object","fields":fields})
        }
        _ => panic!("native fixture output outside reviewed carrier kinds"),
    }
}
pub fn outcome(result: Result<Js, Thrown>, sentinel: &Js) -> Value {
    match result {
        Ok(v) => json!({"ok":true,"value":capture(&v)}),
        Err(e) => {
            json!({"ok":false,"name":e.to_exchange_error().name,"message":e.message(),"sentinel":e.value.strict_equals(sentinel)})
        }
    }
}
#[allow(unused_variables)]
impl crate::websocket_data::WsHost for Host {
    type V = Js;
    type E = Thrown;
    fn get(&mut self, value: &Self::V, key: &[u16]) -> Result<Self::V, Self::E> {
        self.get_value(value, key)
    }
    fn optional(&mut self, value: &Self::V, key: &[u16]) -> Result<Self::V, Self::E> {
        self.optional_value(value, key)
    }
    fn set(&mut self, value: &Self::V, key: &[u16], new: Self::V) -> Result<(), Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::set")
    }
    fn remove(&mut self, value: &Self::V, key: &[u16]) -> Result<(), Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::remove")
    }
    fn truthy(&mut self, value: &Self::V) -> bool {
        value.truthy()
    }
    fn nullish(&mut self, value: &Self::V) -> bool {
        value.is_nullish()
    }
    fn same(&mut self, a: &Self::V, b: &Self::V) -> bool {
        a.strict_equals(b)
    }
    fn set_same(&mut self, a: &Self::V, b: &Self::V) -> bool {
        a.strict_equals(b)
    }
    fn number(&mut self, value: Self::V) -> Result<f64, Self::E> {
        value.to_number()
    }
    fn parse(&mut self, value: Self::V) -> Result<f64, Self::E> {
        value.parse_float()
    }
    fn text(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E> {
        value.to_js_string().map(|s| s.to_vec())
    }
    fn lower(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::lower")
    }
    fn num(&mut self, value: f64) -> Self::V {
        Js::Number(value)
    }
    fn string(&mut self, value: Vec<u16>) -> Self::V {
        Js::utf16(&value)
    }
    fn undefined(&mut self) -> Self::V {
        Js::Undefined
    }
    fn boolean(&mut self, value: bool) -> Self::V {
        Js::Bool(value)
    }
    fn array(&mut self, values: Vec<Self::V>) -> Self::V {
        Js::array(values)
    }
    fn object(&mut self, values: Vec<(&'static str, Self::V)>) -> Self::V {
        Js::object(values)
    }
    fn spread(&mut self, value: &Self::V) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::spread")
    }
    fn rows(&mut self, value: Self::V) -> Result<Vec<Self::V>, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::rows")
    }
    fn values(&mut self, value: &Self::V) -> Result<Vec<Self::V>, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::values")
    }
    fn at(&mut self, value: &Self::V, index: usize) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::at")
    }
    fn is_array(&mut self, value: &Self::V) -> bool {
        panic!("unimplemented accepted fixture capability: WsHost::is_array")
    }
    fn store(&mut self, name: &'static str) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::store")
    }
    fn field(&mut self, name: &'static str) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::field")
    }
    fn call(&mut self, member: &'static str, args: Vec<Self::V>) -> Result<Self::V, Self::E> {
        {
            self.trace
                .borrow_mut()
                .push(json!({"call":member,"args":args.iter().map(capture).collect::<Vec<_>>()}));
            if member == "send" {
                self.sent.borrow_mut().extend(args);
                if self.fail_send {
                    Err(Thrown::new(self.sentinel.clone()))
                } else {
                    Ok(Js::Undefined)
                }
            } else {
                panic!("unexpected fixture call: {member}")
            }
        }
    }
    fn read(
        &mut self,
        endpoint: &'static str,
        params: Self::V,
    ) -> Result<Pin<Box<dyn Future<Output = Result<Self::V, Self::E>>>>, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::read")
    }
    fn spawn(
        &mut self,
        task: Pin<Box<dyn Future<Output = Result<(), Self::E>>>>,
    ) -> Result<(), Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::spawn")
    }
    fn timer(
        &mut self,
        delay: f64,
        task: Box<dyn FnOnce() -> Result<(), Self::E>>,
    ) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::timer")
    }
    fn error_text(&mut self, error: Self::E) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: WsHost::error_text")
    }
}
#[allow(unused_variables)]
impl crate::rest_data::DataHost for Host {
    type V = Js;
    type E = Thrown;
    type Call = ();
    type Read = Ready<Result<Js, Thrown>>;
    fn get(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E> {
        self.get_value(v, &key.encode_utf16().collect::<Vec<_>>())
    }
    fn optional(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E> {
        self.optional_value(v, &key.encode_utf16().collect::<Vec<_>>())
    }
    fn key(&mut self, v: &Self::V, key: &str) -> Result<Self::V, Self::E> {
        {
            self.trace.borrow_mut().push(json!({"mappedOwnKey":key}));
            self.get_value(v, &key.encode_utf16().collect::<Vec<_>>())
        }
    }
    fn truthy(&mut self, v: &Self::V) -> Result<bool, Self::E> {
        Ok(v.truthy())
    }
    fn equals(&mut self, a: &Self::V, b: &Self::V) -> Result<bool, Self::E> {
        Ok(a.strict_equals(b))
    }
    fn is_text(&mut self, v: &Self::V) -> Result<bool, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::is_text")
    }
    fn coerce(&mut self, v: Self::V) -> Result<f64, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::coerce")
    }
    fn number_value(&mut self, v: &Self::V) -> Result<Option<f64>, Self::E> {
        Ok(match v {
            Js::Number(n) => Some(*n),
            _ => None,
        })
    }
    fn parse(&mut self, v: Self::V) -> Result<f64, Self::E> {
        v.parse_float()
    }
    fn text(&mut self, v: Self::V) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::text")
    }
    fn upper(&mut self, text: Vec<u16>) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::upper")
    }
    fn interval_text(&mut self, value: Self::V) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::interval_text")
    }
    fn lower(&mut self, text: Vec<u16>) -> Result<Vec<u16>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::lower")
    }
    fn new_text(&mut self, text: Vec<u16>) -> Self::V {
        Js::utf16(&text)
    }
    fn num(&mut self, n: f64) -> Self::V {
        Js::Number(n)
    }
    fn boolean(&mut self, b: bool) -> Self::V {
        panic!("unimplemented accepted fixture capability: DataHost::boolean")
    }
    fn undefined(&mut self) -> Self::V {
        Js::Undefined
    }
    fn array(&mut self, items: Vec<Self::V>) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::array")
    }
    fn object(&mut self, fields: Vec<(&'static str, Self::V)>) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::object")
    }
    fn spread(
        &mut self,
        v: &Self::V,
        fields: Vec<(&'static str, Self::V)>,
    ) -> Result<Self::V, Self::E> {
        {
            let copy = v.spread_object()?;
            for (key, value) in fields {
                copy.set(key, value)?;
            }
            Ok(copy)
        }
    }
    fn rows(&mut self, v: Self::V) -> Result<Vec<Self::V>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::rows")
    }
    fn is_array(&mut self, v: &Self::V) -> Result<bool, Self::E> {
        Ok(v.is_array())
    }
    fn length(&mut self, v: &Self::V) -> Result<usize, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::length")
    }
    fn at(&mut self, v: &Self::V, index: usize) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::at")
    }
    fn store(&mut self, key: &str) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::store")
    }
    fn cached(&mut self, table: &str, symbol: &Self::V) -> Result<Self::V, Self::E> {
        {
            assert_eq!(table, "marketsBySymbol");
            self.trace
                .borrow_mut()
                .push(json!({"cached":capture(symbol)}));
            Ok(self.market.clone())
        }
    }
    fn format_in(&mut self, symbol: Self::V) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::format_in")
    }
    fn format_out(&mut self, symbol: Self::V) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::format_out")
    }
    fn table(&mut self, table: &str, key: Self::V) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::table")
    }
    fn position_id(
        &mut self,
        symbol: &Self::V,
        side: &str,
        mode: &str,
    ) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::position_id")
    }
    fn price_diff(&mut self, entry: f64, last: f64, side: &str) -> Result<f64, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::price_diff")
    }
    fn map_position(&mut self, row: Self::V) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::map_position")
    }
    fn account_positions(&mut self) -> impl Future<Output = Result<Self::V, Self::E>> {
        std::future::ready(Err(Thrown::type_error(
            "unexpected fixture account_positions",
        )))
    }
    fn symbol_positions(
        &mut self,
        symbol: Self::V,
    ) -> impl Future<Output = Result<Self::V, Self::E>> {
        std::future::ready(Err(Thrown::type_error(
            "unexpected fixture symbol_positions",
        )))
    }
    fn flatten(&mut self, arrays: Vec<Self::V>) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::flatten")
    }
    fn read_target(&mut self) -> Result<Self::Call, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::read_target")
    }
    fn read(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        params: Vec<(&'static str, Self::V)>,
    ) -> Result<Self::Read, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::read")
    }
    fn error_value(&mut self, error: &Self::E) -> Self::V {
        panic!("unimplemented accepted fixture capability: DataHost::error_value")
    }
    fn error(&mut self, message: Self::V) -> Result<Self::E, Self::E> {
        Ok(Thrown::new(Js::object(vec![
            ("name", Js::str("Error")),
            ("message", message),
        ])))
    }
    fn emit(&mut self, message: Self::V) -> Result<(), Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::emit")
    }
    fn emit_candle_error(&mut self, message: Self::V) -> Result<(), Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::emit_candle_error")
    }
    fn now(&mut self) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::now")
    }
    fn includes(&mut self, value: &Self::V, needle: &str) -> Result<bool, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::includes")
    }
    fn nullish(&mut self, v: &Self::V) -> Result<bool, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::nullish")
    }
    fn public_only(&mut self) -> Result<bool, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::public_only")
    }
    fn date_seconds(&mut self, value: Self::V) -> Result<Option<f64>, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::date_seconds")
    }
    fn end_time(&mut self, to: Self::V, daily: bool) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::end_time")
    }
    fn start_time(
        &mut self,
        end: &Self::V,
        from: Self::V,
        use_from: bool,
        amount: f64,
        unit: Vec<u16>,
    ) -> Result<Self::V, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::start_time")
    }
    fn milliseconds(&mut self, date: &Self::V) -> Result<f64, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::milliseconds")
    }
    fn round_usd(&mut self, n: f64) -> Result<f64, Self::E> {
        panic!("unimplemented accepted fixture capability: DataHost::round_usd")
    }
}
#[allow(unused_variables)]
impl crate::rest_account_control::ContextHost for Host {
    type Symbol = Js;
    type Row = Js;
    type Rows = Js;
    type Reply = Js;
    type Call = ();
    type Value = Js;
    type Error = Thrown;
    fn ends_usdc(&mut self, symbol: &Self::Symbol) -> Result<bool, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::ends_usdc")
    }
    fn format_out(&mut self, symbol: &Self::Symbol) -> Result<Self::Value, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::format_out")
    }
    fn get_target(&mut self) -> Result<Self::Call, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::get_target")
    }
    fn get(
        &mut self,
        call: Self::Call,
        endpoint: &'static str,
        currency: &'static str,
        symbol: Self::Value,
    ) -> impl Future<Output = Result<Self::Reply, Self::Error>> {
        std::future::ready(Err(Thrown::type_error("unexpected fixture context read")))
    }
    fn raw_rows(&mut self, reply: Self::Reply) -> Result<Self::Rows, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::raw_rows")
    }
    fn stored_rows(&mut self) -> Result<Self::Rows, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::stored_rows")
    }
    fn length(&mut self, rows: &Self::Rows) -> Result<usize, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::length")
    }
    fn row(&mut self, rows: &Self::Rows, index: usize) -> Result<Option<Self::Row>, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::row")
    }
    fn matches(
        &mut self,
        row: &Self::Row,
        symbol: &Self::Symbol,
        formatted: bool,
    ) -> Result<bool, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::matches")
    }
    fn side_is(
        &mut self,
        row: &Self::Row,
        field: &'static str,
        side: &'static str,
    ) -> Result<bool, Self::Error> {
        {
            self.trace
                .borrow_mut()
                .push(json!({"field":field,"side":side}));
            if row.get("reject")?.truthy() {
                return Err(Thrown::new(self.sentinel.clone()));
            }
            Ok(row.get(field)?.strict_equals(&Js::str(side)))
        }
    }
    fn stored_margin(&mut self, row: &Self::Row) -> Result<Option<Self::Value>, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::stored_margin")
    }
    fn stored_mode(&mut self, row: &Self::Row) -> Result<Option<Self::Value>, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::stored_mode")
    }
    fn stored_leverage(&mut self, row: &Self::Row) -> Result<f64, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::stored_leverage")
    }
    fn raw_leverage(&mut self, row: &Self::Row) -> Result<f64, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::raw_leverage")
    }
    fn cross_margin(&mut self, row: &Self::Row) -> Result<bool, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::cross_margin")
    }
    fn raw_hedged(&mut self, row: &Self::Row) -> Result<bool, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::raw_hedged")
    }
    fn value(&mut self, value: &'static str) -> Result<Self::Value, Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::value")
    }
    fn report(&mut self, error: &Self::Error) -> Result<(), Self::Error> {
        panic!("unimplemented accepted fixture capability: ContextHost::report")
    }
}
