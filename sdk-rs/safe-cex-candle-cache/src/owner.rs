#[cfg(target_arch = "wasm32")]
fn main() {}
#[cfg(not(target_arch = "wasm32"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    host::run()
}
#[cfg(not(target_arch = "wasm32"))]
mod host {
    use safe_cex_candle_cache::{
        CandleCache, CandleCacheBackendReadMode, CandleCacheFetchRequest, CandleCacheOptions,
        FetchOptions,
        native::{RedisCandleCacheBackend, local_runtime},
    };
    use safe_cex_types::{AnyTimeframe, Candle};
    use serde::Deserialize;
    use serde_json::{Value, json};
    use std::{
        cell::{Cell, RefCell},
        collections::HashMap,
        rc::Rc,
    };
    use tokio::{
        io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
        sync::{mpsc, oneshot},
    };
    struct ReplyGuard {
        output: mpsc::UnboundedSender<Value>,
        id: u64,
        finished: bool,
    }
    impl ReplyGuard {
        fn finish(&mut self, response: Value) {
            self.finished = true;
            let _ = self.output.send(response);
        }
    }
    impl Drop for ReplyGuard {
        fn drop(&mut self) {
            if !self.finished {
                let _ = self.output.send(
                    json!({"kind":"result","id":self.id,"error":"Native cache request cancelled"}),
                );
            }
        }
    }
    struct Request {
        cache_key: String,
        interval: AnyTimeframe,
        from_ms: f64,
        to_ms: f64,
        limit: f64,
        now_ms: Option<f64>,
        use_persistent_backend: Option<bool>,
    }
    #[derive(Deserialize)]
    #[serde(remote = "Request", rename_all = "camelCase")]
    struct RequestWire {
        cache_key: String,
        interval: AnyTimeframe,
        from_ms: f64,
        to_ms: f64,
        limit: f64,
        now_ms: Option<f64>,
        use_persistent_backend: Option<bool>,
    }
    impl<'de> Deserialize<'de> for Request {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            RequestWire::deserialize(safe_cex_types::strict_decode::object(d))
        }
    }
    pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        runtime
            .block_on(tokio::task::LocalSet::new().run_until(serve()))
            .map_err(Into::into)
    }
    async fn serve() -> Result<(), String> {
        let detached = Rc::new(RefCell::new(Vec::new()));
        let mut host = local_runtime();
        let spawn_tasks = detached.clone();
        host.spawn = Rc::new(move |future| {
            let mut tasks = spawn_tasks.borrow_mut();
            tasks.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
            tasks.push(tokio::task::spawn_local(future));
        });
        let backend = match std::env::var("REDIS_URL") {
            Ok(url) if !url.is_empty() => Some(Rc::new(
                RedisCandleCacheBackend::connect(&url, None, host.clone(), true).await?,
            )
                as Rc<dyn safe_cex_candle_cache::CandleCacheBackend>),
            _ => None,
        };
        let cache = CandleCache::new(
            CandleCacheOptions {
                backend,
                backend_read_mode: Some(CandleCacheBackendReadMode::ReadThrough),
                ..Default::default()
            },
            host,
        );
        let (output, mut lines_out) = mpsc::unbounded_channel::<Value>();
        let writer = tokio::task::spawn_local(async move {
            let mut stdout = tokio::io::stdout();
            while let Some(message) = lines_out.recv().await {
                let mut bytes = serde_json::to_vec(&message).map_err(|e| e.to_string())?;
                bytes.push(b'\n');
                stdout.write_all(&bytes).await.map_err(|e| e.to_string())?;
                stdout.flush().await.map_err(|e| e.to_string())?;
            }
            Ok::<_, String>(())
        });
        type Pending = Rc<RefCell<HashMap<u64, oneshot::Sender<Result<Vec<Candle>, String>>>>>;
        let pending: Pending = Rc::new(RefCell::new(HashMap::new()));
        let next = Rc::new(Cell::new(0_u64));
        let mut tasks = Vec::new();
        output
            .send(json!({"kind":"ready"}))
            .map_err(|e| e.to_string())?;
        let mut input = BufReader::new(tokio::io::stdin()).lines();
        while let Some(line) = input.next_line().await.map_err(|e| e.to_string())? {
            tasks.retain(|task: &tokio::task::JoinHandle<()>| !task.is_finished());
            let message: Value = serde_json::from_str(&line).map_err(|e| e.to_string())?;
            match message["kind"].as_str() {
                Some("upstreamResult") => {
                    let id = message["id"].as_u64().ok_or("missing upstream id")?;
                    if let Some(sender) = pending.borrow_mut().remove(&id) {
                        let value = if let Some(error) = message["error"].as_str() {
                            Err(error.to_string())
                        } else {
                            serde_json::from_value(message["candles"].clone())
                                .map_err(|e| e.to_string())
                        };
                        let _ = sender.send(value);
                    }
                }
                Some("fetch") => {
                    let id = message["id"].as_u64().ok_or("missing request id")?;
                    let object = message["request"]
                        .as_object()
                        .ok_or("request must be an object")?;
                    let request: Request = serde_json::from_value(Value::Object(object.clone()))
                        .map_err(|e| e.to_string())?;
                    let cache = cache.clone();
                    let output = output.clone();
                    let pending = pending.clone();
                    let next = next.clone();
                    let mut reply = ReplyGuard {
                        output: output.clone(),
                        id,
                        finished: false,
                    };
                    tasks.push(tokio::task::spawn_local(async move {
                        let callback_output = output.clone();
                        let fetcher = Rc::new(move |options: FetchOptions| {
                            let id = next.get(); next.set(id + 1);
                            let (sender, receiver) = oneshot::channel(); pending.borrow_mut().insert(id, sender);
                            let _ = callback_output.send(json!({"kind":"upstream", "id":id, "requestId": message["id"], "options":{"interval":options.interval,"from":options.from,"to":options.to,"limit":options.limit}}));
                            Box::pin(async move { receiver.await.map_err(|_| "upstream channel closed".to_string())? }) as safe_cex_candle_cache::LocalFuture<'static, Result<Vec<Candle>, String>>
                        });
                        let result = cache.fetch(CandleCacheFetchRequest { cache_key:request.cache_key, interval:request.interval, from_ms:request.from_ms, to_ms:request.to_ms, limit:request.limit, now_ms:request.now_ms, use_persistent_backend:request.use_persistent_backend, fetcher }).await;
                        let response = match result { Ok(candles) => json!({"kind":"result","id":id,"candles":candles}), Err(error) => json!({"kind":"result","id":id,"error":error}) };
                        reply.finish(response);
                    }));
                }
                Some("shutdown") => break,
                _ => return Err("unknown owner command".into()),
            }
        }
        // Closing the command stream cancels pending callbacks before awaiting request tasks.
        pending.borrow_mut().clear();
        for task in &tasks {
            task.abort();
        }
        for task in tasks {
            let _ = task.await;
        }
        loop {
            let tasks = std::mem::take(&mut *detached.borrow_mut());
            if tasks.is_empty() {
                break;
            }
            for task in tasks {
                let _ = task.await;
            }
        }
        drop(cache);
        drop(output);
        writer.await.map_err(|e| e.to_string())??;
        Ok::<_, String>(())
    }
    #[test]
    fn owner_value_request_preserves_integer_negative_zero() {
        for field in ["fromMs", "toMs", "limit", "nowMs"] {
            let raw = r#"{"cacheKey":"x","interval":"1m","fromMs":1,"toMs":1,"limit":1,"nowMs":1}"#
                .replace(&format!("\"{field}\":1"), &format!("\"{field}\":-0"));
            let value: Value = serde_json::from_str(&raw).unwrap();
            assert_eq!(
                value[field].as_f64().unwrap().to_bits(),
                (-0.0_f64).to_bits()
            );
            let request: Request = serde_json::from_value(value).unwrap();
            let actual = match field {
                "fromMs" => request.from_ms,
                "toMs" => request.to_ms,
                "limit" => request.limit,
                "nowMs" => request.now_ms.unwrap(),
                _ => unreachable!(),
            };
            assert_eq!(
                actual.to_bits(),
                (-0.0_f64).to_bits(),
                "{field}: owner ingress must retain the sign"
            );
        }
    }
    #[test]
    fn owner_request_preserves_fields_and_refuses_complete_array() {
        use safe_cex_types::Timeframe;
        fn check(request: Request) {
            assert_eq!(request.cache_key, "native-field-key");
            assert_eq!(request.interval, AnyTimeframe::Variant1(Timeframe::M1));
            assert_eq!(request.from_ms.to_bits(), 1.25_f64.to_bits());
            assert_eq!(request.to_ms.to_bits(), 99.5_f64.to_bits());
            assert_eq!(request.limit.to_bits(), 7.75_f64.to_bits());
            assert_eq!(request.now_ms.unwrap().to_bits(), (-0.0_f64).to_bits());
            assert_eq!(request.use_persistent_backend, Some(true));
        }
        let raw = r#"{"cacheKey":"native-field-key","interval":"1m","fromMs":1.25,"toMs":99.5,"limit":7.75,"nowMs":-0,"usePersistentBackend":true}"#;
        let value: Value = serde_json::from_str(raw).unwrap();
        check(serde_json::from_str(raw).unwrap());
        check(serde_json::from_value(value.clone()).unwrap());
        check(Request::deserialize(&value).unwrap());
        check(<Request as Deserialize>::deserialize(value).unwrap());
        let array = r#"["native-field-key","1m",1.25,99.5,7.75,-0,true]"#;
        let value: Value = serde_json::from_str(array).unwrap();
        assert!(
            serde_json::from_str::<Request>(array).is_err(),
            "complete ordered array must be refused by raw request decoder"
        );
        assert!(
            serde_json::from_value::<Request>(value.clone()).is_err(),
            "complete ordered array must be refused by owned request decoder"
        );
        assert!(
            Request::deserialize(&value).is_err(),
            "complete ordered array must be refused by borrowed request decoder"
        );
        assert!(
            <Request as Deserialize>::deserialize(value).is_err(),
            "complete ordered array must be refused by qualified request decoder"
        );
        for invalid in [
            json!([]),
            json!(null),
            json!(true),
            json!(1),
            json!("request"),
            json!({}),
        ] {
            assert!(serde_json::from_str::<Request>(&invalid.to_string()).is_err());
            assert!(serde_json::from_value::<Request>(invalid.clone()).is_err());
            assert!(Request::deserialize(&invalid).is_err());
            assert!(<Request as Deserialize>::deserialize(invalid).is_err());
        }
        for extra in ["", r#", "nowMs":null,"usePersistentBackend":null"#] {
            let raw = format!(
                r#"{{"cacheKey":"missing-optionals","interval":"1m","fromMs":1,"toMs":2,"limit":3{extra}}}"#
            );
            let request: Request = serde_json::from_str(&raw).unwrap();
            assert!(request.now_ms.is_none());
            assert!(request.use_persistent_backend.is_none());
        }
    }
}
