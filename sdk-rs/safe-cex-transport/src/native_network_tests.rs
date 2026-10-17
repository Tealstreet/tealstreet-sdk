use super::*;
use crate::rate_limit::RateLimitClock;
use crate::retry::{ClassifiedRequest, ReadClassification, network_error_on_read};
use std::{
    io::Read,
    process::Command,
    sync::atomic::{AtomicUsize, Ordering},
};

struct Clock(Cell<usize>);
impl RateLimitClock for Clock {
    fn now_ms(&self) -> u64 {
        1
    }
    fn sleep_ms(&self, _: u64) -> TransportFuture<'_, ()> {
        self.0.set(self.0.get() + 1);
        Box::pin(async { Ok(()) })
    }
}
#[derive(Debug)]
struct MissingHost {
    calls: Arc<AtomicUsize>,
    kind: std::io::ErrorKind,
    message: &'static str,
}
impl reqwest::dns::Resolve for MissingHost {
    fn resolve(&self, _: reqwest::dns::Name) -> reqwest::dns::Resolving {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let (kind, message) = (self.kind, self.message);
        Box::pin(async move { Err(std::io::Error::new(kind, message).into()) })
    }
}
fn run(future: impl Future<Output = ()>) {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(future);
}
async fn rejected(adapter: &ReqwestAdapter, url: String, code: &str) {
    let request = RestRequest {
        method: "GET".into(),
        url: url.clone(),
        timeout_ms: Some(2_000),
        ..Default::default()
    };
    let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(request) else {
        panic!("GET read");
    };
    let clock = Clock(Cell::new(0));
    let error = network_error_on_read(read, 3)
        .execute(adapter, &clock)
        .await
        .unwrap_err();
    assert_eq!(
        error.code.as_deref(),
        Some(code),
        "NATIVE-NETWORK originating code: {error:?}"
    );
    assert!(
        !error.is_retryable_network(),
        "NATIVE-NETWORK denied failure must not retry"
    );
    assert_eq!(clock.0.get(), 0, "NATIVE-NETWORK no retry scheduling");
    assert_eq!(error.request.url, url);
}
#[test]
fn dns_failure_keeps_resolution_code_and_does_not_retry() {
    run(async {
        let calls = Arc::new(AtomicUsize::new(0));
        let adapter = ReqwestAdapter {
            client: reqwest::Client::builder()
                .tls_backend_preconfigured((*tls_config().unwrap()).clone())
                .no_proxy()
                .dns_resolver(Arc::new(MissingHost {
                    calls: calls.clone(),
                    kind: std::io::ErrorKind::NotFound,
                    message: "host not found",
                }))
                .build()
                .unwrap(),
        };
        rejected(
            &adapter,
            "http://missing.invalid/snapshot".into(),
            "ENOTFOUND",
        )
        .await;
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "NATIVE-NETWORK one DNS attempt"
        );
    });
}
#[test]
fn temporary_dns_and_unreachable_network_keep_distinct_retry_policy() {
    run(async {
        for (kind, message, code, retryable, attempts) in [
            (
                std::io::ErrorKind::WouldBlock,
                "Temporary failure in name resolution",
                "EAI_AGAIN",
                true,
                4,
            ),
            (
                std::io::ErrorKind::NetworkUnreachable,
                "Network is unreachable",
                "ENETUNREACH",
                false,
                1,
            ),
        ] {
            let calls = Arc::new(AtomicUsize::new(0));
            let adapter = ReqwestAdapter {
                client: reqwest::Client::builder()
                    .tls_backend_preconfigured((*tls_config().unwrap()).clone())
                    .no_proxy()
                    .dns_resolver(Arc::new(MissingHost {
                        calls: calls.clone(),
                        kind,
                        message,
                    }))
                    .build()
                    .unwrap(),
            };
            let request = RestRequest {
                method: "GET".into(),
                url: "http://missing.invalid/snapshot".into(),
                ..Default::default()
            };
            let ClassifiedRequest::Read(read) = ReadClassification::Methods.classify(request)
            else {
                panic!("GET read");
            };
            let clock = Clock(Cell::new(0));
            let error = network_error_on_read(read, 3)
                .execute(&adapter, &clock)
                .await
                .unwrap_err();
            assert_eq!(error.code.as_deref(), Some(code));
            assert_eq!(error.is_retryable_network(), retryable);
            assert_eq!(
                calls.load(Ordering::SeqCst),
                attempts,
                "NATIVE-NETWORK resolver attempt count"
            );
            assert_eq!(clock.0.get(), attempts - 1);
        }
    });
}
#[test]
fn refused_connection_remains_retryable() {
    run(async {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        drop(listener);
        let adapter = ReqwestAdapter {
            client: reqwest::Client::builder()
                .tls_backend_preconfigured((*tls_config().unwrap()).clone())
                .no_proxy()
                .build()
                .unwrap(),
        };
        let error = adapter
            .execute(RestRequest {
                method: "GET".into(),
                url: format!("http://{address}/snapshot"),
                timeout_ms: Some(2_000),
                ..Default::default()
            })
            .await
            .unwrap_err();
        assert_eq!(error.code.as_deref(), Some("ECONNREFUSED"));
        assert!(error.is_retryable_network());
    });
}
#[test]
fn expired_certificate_keeps_tls_code_and_does_not_retry() {
    certificate_failure("expired.der", true, "localhost", "CERT_HAS_EXPIRED", false);
}
#[test]
fn unknown_issuer_keeps_certificate_refusal_category() {
    certificate_failure("valid.der", false, "localhost", "CERT_REJECTED", false);
}
#[test]
fn hostname_mismatch_keeps_the_installed_node_retry_category() {
    certificate_failure(
        "valid.der",
        true,
        "wrong.test",
        "ERR_TLS_CERT_ALTNAME_INVALID",
        true,
    );
}
fn certificate_failure(
    certificate: &str,
    trust_issuer: bool,
    hostname: &str,
    expected_code: &str,
    retryable: bool,
) {
    run(async {
        let directory = std::env::temp_dir().join(format!(
            "safe-cex-network-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&directory).unwrap();
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/native-network-source.cjs"
            ))
            .arg("--certificates")
            .arg(&directory)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "certificate setup: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let root = std::fs::read(directory.join("root.der")).unwrap();
        let cert = std::fs::read(directory.join(certificate)).unwrap();
        let key = std::fs::read(directory.join("key.der")).unwrap();
        std::fs::remove_dir_all(&directory).unwrap();
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let server = rustls::ServerConfig::builder_with_provider(provider.clone())
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(
                vec![cert.into()],
                rustls::pki_types::PrivatePkcs8KeyDer::from(key).into(),
            )
            .unwrap();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let thread = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            stream
                .set_write_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let connection = rustls::ServerConnection::new(Arc::new(server)).unwrap();
            let mut stream = rustls::StreamOwned::new(connection, stream);
            let error = stream
                .read(&mut [0; 1])
                .expect_err("client rejects certificate before HTTP");
            assert_ne!(error.kind(), std::io::ErrorKind::WouldBlock);
        });
        let mut roots = rustls::RootCertStore::empty();
        if trust_issuer {
            roots.add(root.into()).unwrap();
        }
        let tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let adapter = ReqwestAdapter {
            client: reqwest::Client::builder()
                .no_proxy()
                .tls_backend_preconfigured(tls)
                .resolve(hostname, address)
                .build()
                .unwrap(),
        };
        let url = format!("https://{hostname}:{}/snapshot", address.port());
        if retryable {
            let error = adapter
                .execute(RestRequest {
                    method: "GET".into(),
                    url,
                    timeout_ms: Some(2_000),
                    ..Default::default()
                })
                .await
                .unwrap_err();
            assert_eq!(error.code.as_deref(), Some(expected_code));
            assert!(error.is_retryable_network());
        } else {
            rejected(&adapter, url, expected_code).await;
        }
        thread.join().unwrap();
    });
}
