//! Exact EEA demo requests must prove both possession and simulated-trading scope.
use super::*;

#[derive(Clone, Copy, Debug)]
enum HeaderControl {
    Native,
    Missing,
    Mainnet,
    Duplicate,
    Unsigned,
}

#[derive(Clone)]
struct DemoIo {
    io: VenueIo,
    control: HeaderControl,
    requests: Arc<Mutex<Vec<HttpRequest>>>,
}

impl Transport for DemoIo {
    fn prepare_http(
        &self,
        _: HttpRequest,
        _: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        let io = self.clone();
        Box::pin(async move { Ok(Box::new(Prepared(io)) as Box<dyn PreparedHttp>) })
    }

    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        assert!(url.starts_with("wss://wseeapap.okx.com/ws/v5/"));
        self.io.connect(socket, url, events)
    }

    fn request(&self, mut request: HttpRequest, events: Events) {
        assert_eq!(request.method, Method::Get, "admission never trades");
        assert!(request.body.is_none());
        assert!(request.url.starts_with("https://eea.okx.com/"));
        self.requests.lock().unwrap().push(request.clone());
        if self.io.authenticated(&request).is_some() {
            match self.control {
                HeaderControl::Native => {}
                HeaderControl::Missing => request
                    .headers
                    .retain(|(name, _)| !name.eq_ignore_ascii_case("x-simulated-trading")),
                HeaderControl::Mainnet => {
                    for (name, value) in &mut request.headers {
                        if name.eq_ignore_ascii_case("x-simulated-trading") {
                            *value = "0".into();
                        }
                    }
                }
                HeaderControl::Duplicate => request
                    .headers
                    .push(("x-simulated-trading".into(), "1".into())),
                HeaderControl::Unsigned => request
                    .headers
                    .retain(|(name, _)| !name.eq_ignore_ascii_case("OK-ACCESS-SIGN")),
            }
        }
        self.io.request(request, events);
    }
}

#[tokio::test]
async fn okxeudemo_native_proof_requires_exact_demo_header_and_signature() {
    tokio::task::LocalSet::new()
        .run_until(async {
            for control in [
                HeaderControl::Native,
                HeaderControl::Missing,
                HeaderControl::Mainnet,
                HeaderControl::Duplicate,
                HeaderControl::Unsigned,
            ] {
                let seen = Arc::new(Mutex::new(Seen::default()));
                let requests = Arc::new(Mutex::new(Vec::new()));
                let io = DemoIo {
                    io: VenueIo {
                        venue: "okxeudemo",
                        seen: seen.clone(),
                    },
                    control,
                    requests: requests.clone(),
                };
                let prepared = local("okxeudemo", SECRET);
                let result = tokio::time::timeout(
                    Duration::from_secs(3),
                    run(
                        "okxeudemo",
                        &prepared.options,
                        prepared.resident.provider(),
                        io,
                        ServerConfig::default(),
                        Arc::new(|| true),
                    ),
                )
                .await
                .expect("bounded demo proof");
                let seen = seen.lock().unwrap();
                if matches!(control, HeaderControl::Native) {
                    assert!(result.is_ok(), "{result:?}");
                    assert!(seen.signed_reads > 0);
                    assert_eq!(seen.refused, 0);
                    assert_variant_startup("okxeudemo", &seen);
                } else {
                    assert!(
                        matches!(
                            result,
                            Err(ProbeRefusal {
                                transient: false,
                                ..
                            })
                        ),
                        "{control:?}: {result:?}"
                    );
                    assert_eq!(
                        seen.refused, 2,
                        "one re-signed retry, then permanent refusal"
                    );
                    assert!(
                        seen.urls
                            .iter()
                            .all(|url| url.ends_with("/api/v5/account/config")),
                        "no discovery after config refusal"
                    );
                }
                for request in requests.lock().unwrap().iter() {
                    let simulated: Vec<_> = request
                        .headers
                        .iter()
                        .filter(|(name, _)| name.eq_ignore_ascii_case("x-simulated-trading"))
                        .map(|(_, value)| value.as_str())
                        .collect();
                    assert_eq!(simulated, ["1"], "actual native packet before alteration");
                }
            }
        })
        .await;
}
