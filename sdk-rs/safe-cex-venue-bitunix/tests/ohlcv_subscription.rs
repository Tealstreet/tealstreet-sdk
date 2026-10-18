use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{JsJsonValue, js_own_keys};
use safe_cex_types::Candle;
use safe_cex_venue_bitunix::ohlcv_subscription::*;
use serde_json::{Value, json};
use std::{cell::RefCell, process::Command, rc::Rc};
fn bits(n: f64) -> Value {
    json!({"bits":format!("{:016x}",if n.is_nan(){0x7ff8_0000_0000_0000}else{n.to_bits()})})
}
fn text(s: &str) -> Value {
    json!({"units":s.encode_utf16().collect::<Vec<_>>()})
}
fn undefined() -> Value {
    json!({"undefined":true})
}
fn obs(v: &JsJsonValue) -> Value {
    match v {
        JsJsonValue::Undefined => undefined(),
        JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(b) => json!(b),
        JsJsonValue::Number(n) => bits(*n),
        JsJsonValue::String(s) => text(s),
        JsJsonValue::Array(a) => json!(a.iter().map(obs).collect::<Vec<_>>()),
        JsJsonValue::Object(o) => {
            json!({"entries":o.iter().map(|(k,v)|json!([k,obs(v)])).collect::<Vec<_>>()})
        }
        JsJsonValue::BigInt(_) => panic!("unexpected fixture BigInt"),
    }
}
fn value_obs(v: &Value) -> Value {
    match v {
        Value::Object(o) => {
            json!({"entries":js_own_keys(o.keys().map(String::as_str)).into_iter().map(|k|json!([k,value_obs(&o[k])])).collect::<Vec<_>>()})
        }
        _ => obs(&JsJsonValue::from(v.clone())),
    }
}
fn candle_obs(c: &Candle) -> Value {
    json!({"entries":[["timestamp",bits(c.timestamp)],["open",bits(c.open)],["high",bits(c.high)],["low",bits(c.low)],["close",bits(c.close)],["volume",bits(c.volume)]]})
}
struct Host {
    row: Value,
    events: Vec<Value>,
    connected: bool,
    handlers: Option<Vec<usize>>,
    count: Option<f64>,
    last: Option<f64>,
    allocated: usize,
    callback_count: usize,
    waits: usize,
    original_symbol: String,
    raw_time: Value,
}
impl Host {
    fn note(&mut self, name: &str, args: Vec<Value>) -> Result<(), ExchangeError> {
        let mut event = vec![json!(name)];
        event.extend(args);
        let event = json!(event);
        let i = self.events.len();
        assert_eq!(
            event, self.row["events"][i],
            "CANDLE native effect {name}:{i}"
        );
        self.events.push(event);
        if self.row["fixture"]["fail"] == name {
            Err(ExchangeError::new(format!("{name} failed")))
        } else {
            Ok(())
        }
    }
    fn handler_obs(v: &Option<Vec<usize>>) -> Value {
        v.as_ref().map_or_else(undefined, |ids| {
            json!(
                ids.iter()
                    .map(|n| text(&if *n == usize::MAX {
                        "other".to_owned()
                    } else {
                        format!("p{n}")
                    }))
                    .collect::<Vec<_>>()
            )
        })
    }
}
impl OhlcvHost<usize> for Host {
    type Parser = usize;
    type Ticker = Rc<()>;
    fn allocate_parser(&mut self) -> usize {
        let id = self.allocated;
        self.allocated += 1;
        id
    }
    fn connected(&mut self) -> Result<bool, ExchangeError> {
        self.note("connected", vec![json!(self.connected)])?;
        Ok(self.connected)
    }
    fn handlers(&mut self, handler: &str) -> Result<Vec<usize>, ExchangeError> {
        self.note(
            "handlers",
            vec![text(handler), Self::handler_obs(&self.handlers)],
        )?;
        Ok(self.handlers.clone().unwrap_or_default())
    }
    fn set_handlers(&mut self, handler: &str, handlers: Vec<usize>) -> Result<(), ExchangeError> {
        self.note(
            "setHandlers",
            vec![text(handler), Self::handler_obs(&Some(handlers.clone()))],
        )?;
        self.handlers = Some(handlers);
        Ok(())
    }
    fn topic_count(&mut self, topic: &str) -> Result<f64, ExchangeError> {
        self.note(
            "count",
            vec![text(topic), self.count.map_or_else(undefined, bits)],
        )?;
        Ok(self.count.unwrap_or(f64::NAN))
    }
    fn set_topic_count(&mut self, topic: &str, count: f64) -> Result<(), ExchangeError> {
        self.note("setCount", vec![text(topic), bits(count)])?;
        self.count = Some(count);
        Ok(())
    }
    fn send(&mut self, frame: &JsJsonValue) -> Result<(), ExchangeError> {
        self.note("send", vec![obs(frame)])
    }
    fn wait(&mut self, delay: f64) -> Result<(), ExchangeError> {
        self.note("wait", vec![bits(delay)])?;
        self.waits += 1;
        Ok(())
    }
    fn cleanup_stats(&mut self, handler: &str) -> Result<(), ExchangeError> {
        self.note("cleanup", vec![text(handler)])
    }
    fn ticker(&mut self, symbol: &str) -> Result<Option<Rc<()>>, ExchangeError> {
        self.note("find", vec![text(symbol)])?;
        Ok(
            (self.row["fixture"]["ticker"] != false && self.original_symbol == symbol)
                .then(|| Rc::new(())),
        )
    }
    fn update_last(&mut self, ticker: Rc<()>, last: f64) -> Result<(), ExchangeError> {
        assert_eq!(
            Rc::strong_count(&ticker),
            1,
            "CANDLE native opaque ticker ownership"
        );
        self.note("updateLast", vec![bits(last)])?;
        self.last = Some(last);
        Ok(())
    }
    fn call(&mut self, callback: &usize, candle: Candle) -> Result<(), ExchangeError> {
        let mut expected = self.row["sourceCandles"][self.callback_count].clone();
        let expected_time = self.row["nativeTimestamps"][self.callback_count].clone();
        self.callback_count += 1;
        assert_eq!(
            bits(candle.timestamp),
            expected_time,
            "CANDLE native timestamp"
        );
        expected["entries"][0][1] = bits(candle.timestamp);
        assert_eq!(candle_obs(&candle), expected, "CANDLE native fields");
        assert!(candle.last_side.is_none(), "CANDLE native absent last side");
        let mut source = candle_obs(&candle);
        source["entries"][0][1] = self.raw_time.clone();
        self.note("callback", vec![bits(*callback as f64), source])
    }
}
#[test]
fn actual_typescript_ohlcv_subscription() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/ohlcv-subscription-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CANDLE actual source {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    let rows = corpus["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 37, "CANDLE independent source floor");
    let mut boundary_count = 0;
    let mut callbacks = 0;
    for row in rows {
        let f = &row["fixture"];
        let symbol = f["symbol"].as_str().unwrap_or("BTCUSDT");
        let opts = Rc::new(RefCell::new(OhlcvOptions {
            symbol: symbol.into(),
            interval: f["interval"].as_str().unwrap_or("1m").into(),
        }));
        let mut host = Host {
            row: row.clone(),
            events: vec![],
            connected: f["connected"] != false,
            handlers: if f["existing"] == true {
                Some(vec![usize::MAX])
            } else {
                None
            },
            count: if f["nanCount"] == true {
                Some(f64::NAN)
            } else {
                f["topicCount"].as_f64()
            },
            last: None,
            allocated: 0,
            callback_count: 0,
            waits: 0,
            original_symbol: symbol.into(),
            raw_time: undefined(),
        };
        let raw=f.get("raw").cloned().unwrap_or_else(||json!({"data":{"time":1700000000000i64,"open":"-0","high":"2e2x","low":"  .5","close":"123.4tail","volume":"Infinity"}}));
        host.raw_time = raw
            .get("data")
            .and_then(|d| d.get("time"))
            .map_or_else(undefined, value_obs);
        let result = (|| {
            let sub = listen_ohlcv(&mut host, opts.clone(), 0)?;
            assert_eq!(
                text(&sub.handler),
                text(row["handler"].as_str().unwrap()),
                "CANDLE native channel"
            );
            assert_eq!(
                sub.topic, row["topic"],
                "CANDLE frame string after native channel"
            );
            assert!(
                Rc::ptr_eq(&sub.options, &opts),
                "CANDLE native option identity"
            );
            let second = if f["second"] == true {
                Some(listen_ohlcv(&mut host, opts.clone(), 1)?)
            } else {
                None
            };
            if let Some(symbol) = f["mutate"].as_str() {
                opts.borrow_mut().symbol = symbol.into();
            }
            let default = json!(["parse", "cancel"]);
            let actions = f.get("actions").unwrap_or(&default).as_array().unwrap();
            for action in actions {
                match action.as_str().unwrap() {
                    "connect" => host.connected = true,
                    "retry" => {
                        if host.waits > 0 {
                            host.waits -= 1;
                            sub.subscribe(&mut host)?;
                        }
                    }
                    "cancel" => sub.cancel(&mut host)?,
                    "parse" => {
                        for id in host.handlers.clone().unwrap_or_default() {
                            if id == 0 {
                                sub.parse(&mut host, &raw)?;
                            } else if id == 1 {
                                second.as_ref().unwrap().parse(&mut host, &raw)?;
                            }
                        }
                    }
                    _ => panic!("unknown witness action"),
                }
            }
            Ok::<_, ExchangeError>(())
        })();
        if let Some(boundary) = f["boundary"].as_str() {
            assert_eq!(
                result.as_ref().unwrap_err().message,
                boundary,
                "CANDLE native malformed boundary"
            );
            assert!(
                row["result"].get("error").is_some(),
                "CANDLE measured source malformed error"
            );
            boundary_count += 1;
        } else {
            let result = match result {
                Ok(()) => json!({"ok":true}),
                Err(e) => json!({"error":{"error":["Error",e.message]}}),
            };
            assert_eq!(result, row["result"], "CANDLE native result");
        }
        assert_eq!(
            host.events.len(),
            row["events"].as_array().unwrap().len(),
            "CANDLE native effect count"
        );
        assert_eq!(
            Host::handler_obs(&host.handlers),
            row["state"]
                .get("handlers")
                .map(value_obs)
                .unwrap_or_else(undefined),
            "CANDLE native handler state"
        );
        assert_eq!(
            host.count.map_or_else(undefined, bits),
            row["state"]["count"],
            "CANDLE native count state"
        );
        assert_eq!(
            host.last.map_or_else(undefined, bits),
            row["state"]["last"],
            "CANDLE native last state"
        );
        callbacks += host.callback_count;
    }
    assert_eq!(
        boundary_count, 3,
        "CANDLE independent malformed boundary floor"
    );
    assert!(callbacks >= 28, "CANDLE independent callback floor");
}
