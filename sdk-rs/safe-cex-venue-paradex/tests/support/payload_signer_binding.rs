use super::*;
use safe_cex_crypto::{
    Encoding, PayloadScheme, PayloadSigner, PayloadSignerRequest, PayloadSignerResult,
};
use safe_cex_exchange::{
    js_runtime::{client, value::to_json},
    native::native_runtime,
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    future::Future,
    io::{Read, Write},
    net::TcpListener,
    pin::Pin,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

struct Signer {
    requests: RefCell<Vec<Value>>,
    reject: Cell<bool>,
}
impl PayloadSigner for Signer {
    fn sign<'a>(
        &'a self,
        r: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>> {
        let (scheme, encoding, signature) = match r.scheme {
            PayloadScheme::HmacSha256 { encoding } => (
                "hmac-sha256",
                match encoding {
                    Encoding::Hex => "hex",
                    Encoding::Base64 => "base64",
                },
                Ok(safe_cex_crypto::hmac_sha256(
                    b"host-only-secret",
                    r.payload.as_bytes(),
                    encoding,
                )),
            ),
            PayloadScheme::Stark => (
                "stark",
                "felt-pair",
                (|| {
                    let hash = safe_cex_crypto::starknet::parse_felt(r.payload)?;
                    let key = safe_cex_crypto::starknet::parse_felt("0x123")?;
                    let (r, s) = safe_cex_crypto::starknet::sign(&hash, &key)?;
                    Ok::<_, String>(json!([r.to_string(), s.to_string()]).to_string())
                })(),
            ),
        };
        self.requests.borrow_mut().push(json!({"exchangeName":r.exchange_name,"key":r.key,"scheme":scheme,"encoding":encoding,"payload":r.payload}));
        let reject = self.reject.get();
        Box::pin(async move {
            tokio::task::yield_now().await;
            if reject {
                return Err("provider declined".into());
            }
            Ok(PayloadSignerResult {
                signature: signature?,
                headers: Some(BTreeMap::from([(
                    "x-provider-route".into(),
                    "retained".into(),
                )])),
            })
        })
    }
}
struct OriginAdapter {
    wire: Arc<dyn safe_cex_transport::RestAdapter>,
    origin: String,
}
impl safe_cex_transport::RestAdapter for OriginAdapter {
    fn execute(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        let (_, path) = request
            .url
            .split_once(".paradex.trade")
            .expect("actual Paradex API origin");
        request.url = format!("{}{path}", self.origin);
        self.wire.execute(request)
    }
}
struct Loopback {
    url: String,
    requests: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Loopback {
    fn new() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let requests = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (seen, stopped) = (requests.clone(), stop.clone());
        let thread = thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(20);
            while !stopped.load(Ordering::Relaxed) && Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        let mut bytes = Vec::new();
                        let mut buffer = [0; 1024];
                        while !bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                            let n = stream.read(&mut buffer).unwrap();
                            assert_ne!(n, 0);
                            bytes.extend_from_slice(&buffer[..n]);
                        }
                        let text = String::from_utf8(bytes).unwrap();
                        let mut lines = text.split("\r\n");
                        let line = lines.next().unwrap();
                        let mut parts = line.split_whitespace();
                        let method = parts.next().unwrap();
                        let path = parts.next().unwrap();
                        let headers: BTreeMap<_, _> = lines
                            .take_while(|line| !line.is_empty())
                            .map(|line| {
                                let (k, v) = line.split_once(':').unwrap();
                                (k.to_ascii_lowercase(), v.trim().to_owned())
                            })
                            .collect();
                        seen.lock()
                            .unwrap()
                            .push(json!({"method":method,"path":path,"headers":headers}));
                        let body = if path == "/v1/system/config" {
                            assert_eq!(method, "GET");
                            json!({"starknet_fullnode_rpc_url":"http://unused.invalid","starknet_chain_id":"0x534e5f5345504f4c4941","l1_chain_id":"1","paraclear_account_hash":"0x1","paraclear_account_proxy_hash":"0x2","paraclear_address":"0x3","paraclear_decimals":8,"bridged_tokens":[]}).to_string()
                        } else {
                            assert_eq!(path, "/v1/auth/0x456");
                            assert_eq!(method, "POST");
                            json!({"jwt_token":"loopback-token"}).to_string()
                        };
                        write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",body.len(),body).unwrap();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => panic!("loopback accept: {error}"),
                }
            }
        });
        Self {
            url,
            requests,
            stop,
            thread: Some(thread),
        }
    }
}
impl Drop for Loopback {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Err(error) = self.thread.take().unwrap().join() {
            if thread::panicking() {
                eprintln!("loopback server also panicked: {error:?}");
            } else {
                std::panic::resume_unwind(error);
            }
        }
    }
}

#[test]
fn native_payload_provider_signs_real_paradex_order_modify_and_jwt() {
    let executor = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&executor,async {
        let server=Loopback::new();
        let runtime=native_runtime();
        let signer=Rc::new(Signer {requests:RefCell::new(Vec::new()),reject:Cell::new(false)});
        #[expect(clippy::arc_with_non_send_sync, reason="the production RestAdapter port is local-task compatible")]
        let adapter=Arc::new(OriginAdapter {wire:Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),origin:server.url.clone()});
        let host=client::rest_http_host_with_signers(runtime.clone(),adapter,None,Some(signer.clone()));
        let venue=ParadexExchange::new(ExchangeName::Paradex,
            serde_json::from_value(json!({"key":"hosted-key-reference","secret":"","paradexAddress":"0x1234","subkeyAddress":"0x456"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            ParadexDependencies {runtime,http:Rc::new(move |_|Ok(host.clone())),external_public_data:None,local_wall_time_to_utc:Rc::new(|v|v)}).unwrap();
        let options=venue.rt.this.get("options").unwrap();
        assert_eq!(options.get("key").unwrap().to_js_string().unwrap().as_ref(),"hosted-key-reference".encode_utf16().collect::<Vec<_>>());
        let pending=venue.rt.this.get("sdkInitPromise").unwrap();
        assert!(matches!(&pending,Js::Promise(_)),"constructor starts the real SDK initialization");
        let initialized=await_js(&venue.rt.exchange_runtime(),pending).await;
        assert_eq!(server.requests.lock().unwrap()[0]["path"],"/v1/system/config");
        assert!(initialized.is_ok(),"real SDK initialization must use the native payload provider without treating its key selector as a private key: {:?}",initialized.err().map(|e|e.message()));
        let request_signer=venue.rt.this.get("requestSigner").unwrap();
        let parameters=json!({"id":"123456","market":"BTC-USD-PERP","side":"BUY","type":"LIMIT","size":"0.01234567","price":"61234.56789"});
        let mut outputs=Vec::new(); let mut clocks=Vec::new();
        for method in ["signOrder","signModifyOrder"] {
            let result=await_js(&venue.rt.exchange_runtime(),request_signer.call_method(method,vec![from_typed(&parameters).unwrap()]).unwrap()).await;
            assert!(result.is_ok(),"actual {method} must invoke the native payload provider: {:?}",result.err().map(|e|e.message()));
            let result=to_json(&result.unwrap()).unwrap(); clocks.push(result["signature_timestamp"].as_f64().unwrap()); outputs.push(result);
        }
        let manager=venue.rt.this.get("jwtManager").unwrap();
        let token=await_js(&venue.rt.exchange_runtime(),manager.call_method("generateNewToken",vec![]).unwrap()).await.unwrap();
        assert_eq!(token.to_js_string().unwrap().as_ref(),"loopback-token".encode_utf16().collect::<Vec<_>>());
        let native_http=server.requests.lock().unwrap()[1].clone();
        clocks.push(native_http["headers"]["paradex-timestamp"].as_str().unwrap().parse::<f64>().unwrap()*1000.);
        let input=json!({"url":server.url,"parameters":parameters,"clocks":clocks});
        let mut process=Command::new("node").arg(concat!(env!("CARGO_MANIFEST_DIR"),"/tests/payload-signer-binding-oracle.cjs"))
            .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
        process.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
        let output=process.wait_with_output().unwrap();
        assert!(output.status.success(),"Node SDK: {}",String::from_utf8_lossy(&output.stderr));
        let source:Value=serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(source["requests"],json!(*signer.requests.borrow()));
        assert_eq!(source["outputs"].as_array().unwrap().len(),outputs.len());
        for (native,source) in outputs.iter().zip(source["outputs"].as_array().unwrap()) {
            assert_eq!(native.as_object().unwrap().len(),3);
            assert_eq!(source.as_object().unwrap().len(),3);
            assert_eq!(native["signature"],source["signature"]);
            assert_eq!(native["headers"],source["headers"]);
            assert_eq!(native["signature_timestamp"].as_f64().unwrap().to_bits(),source["signature_timestamp"].as_f64().unwrap().to_bits());
        }
        assert_eq!(source["token"],"loopback-token");
        {
        let http=server.requests.lock().unwrap(); assert_eq!(http.len(),3);
        assert_eq!(http[0]["path"],"/v1/system/config");
        for field in ["method","path"] {assert_eq!(http[1][field],http[2][field]);}
        for header in ["content-type","accept","x-provider-route","paradex-starknet-account","paradex-starknet-signature","paradex-timestamp","paradex-signature-expiration"] {
            assert!(http[1]["headers"][header].is_string(),"native header {header}");
            assert_eq!(http[1]["headers"][header],http[2]["headers"][header],"actual JWT {header}");
        }
        }
        let mut hmac=Vec::new();
        for encoding in ["hex","base64"] {
            let request=from_typed(&json!({"exchangeName":"bybit","key":"hosted-key-reference","scheme":"hmac-sha256","encoding":encoding,"payload":"native HMAC control"})).unwrap();
            let result=await_js(&venue.rt.exchange_runtime(),options.call_method("payloadSigner",vec![request]).unwrap()).await.unwrap();
            hmac.push(to_json(&result).unwrap());
        }
        assert_eq!(json!(hmac),source["hmacOutputs"]);
        assert_eq!(json!(&signer.requests.borrow()[3..]),source["hmacRequests"]);
        assert!(venue.base().signers.payload_signer.as_ref().is_some_and(|v|Rc::ptr_eq(v,&(signer.clone() as Rc<dyn PayloadSigner>))));
        signer.reject.set(true);
        let failure=await_js(&venue.rt.exchange_runtime(),request_signer.call_method("signOrder",vec![from_typed(&parameters).unwrap()]).unwrap()).await.unwrap_err();
        assert_eq!(failure.message(),"provider declined");
        assert_eq!(server.requests.lock().unwrap().len(),3,"failed signing performs no HTTP");
        venue.dispose().unwrap();
    });
}
