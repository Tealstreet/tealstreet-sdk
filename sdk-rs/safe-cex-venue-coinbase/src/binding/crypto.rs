use super::error::failure;
use crate::{
    cdp_signing::{CdpAsync, CdpCryptoKey, CdpImportKey, CdpSigningHost, CdpSigningInput},
    portfolios_balances::PortfolioFailure,
    starbase_auth::{CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner},
};
use safe_cex_crypto::es256::Es256SigningKey;
use safe_cex_exchange::{
    js_runtime::{promise::Promise as JsPromise, value::Js},
    runtime::{ExchangeError, ExchangeErrorOrigin, ExchangeRuntime},
};
use std::rc::Rc;

pub(super) fn spawner(runtime: Rc<dyn ExchangeRuntime>) -> CoinbaseStarbasePromiseSpawner {
    Rc::new(move |task| {
        // Reuse the core's eager producer/continuation ownership rather than
        // introducing a venue-specific executor or tying progress to an observer.
        JsPromise::spawn(
            &runtime,
            Box::pin(async move {
                task.await;
                Ok(Js::Undefined)
            }),
        );
    })
}

// Adapt the existing shared source-UUID helper to the host entropy capability.
pub(super) fn uuid(runtime: &dyn ExchangeRuntime) -> String {
    safe_cex_crypto::uuid::uuid_with_entropy(|bytes| {
        for byte in bytes {
            *byte = (runtime.random() * 64.0) as u8;
        }
        Ok(())
    })
    .expect("runtime entropy fill is infallible")
}

pub(super) struct CryptoHost {
    pub runtime: Rc<dyn ExchangeRuntime>,
}

impl CdpSigningHost for CryptoHost {
    fn prepare_pkcs8(&self, secret: &str) -> Result<Vec<u8>, PortfolioFailure> {
        crate::utils::pem_to_pkcs8_der_with(secret, |encoded| {
            safe_cex_transport::codec::decode_base64(encoded).map_err(|original| {
                let mut error = ExchangeError::new(original.to_string());
                error.name = "InvalidCharacterError".into();
                error.origin = Some(ExchangeErrorOrigin::new(original));
                failure(error)
            })
        })
    }
    fn import_key(&self, request: CdpImportKey) -> CdpAsync<CdpCryptoKey> {
        if (
            request.format,
            request.name,
            request.named_curve,
            request.extractable,
            request.usages.as_slice(),
        ) != ("pkcs8", "ECDSA", "P-256", false, ["sign"].as_slice())
        {
            return Err(failure(ExchangeError::new(
                "Unsupported Coinbase key import",
            )));
        }
        let result = Es256SigningKey::from_pkcs8_der(&request.der)
            .map(|key| Rc::new(key) as CdpCryptoKey)
            .map_err(|message| failure(ExchangeError::new(message)));
        Ok(Promise::resolved(result))
    }
    fn current_time_ms(&self) -> Result<f64, PortfolioFailure> {
        Ok(safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms())
    }
    fn uuid(&self) -> Result<String, PortfolioFailure> {
        Ok(uuid(self.runtime.as_ref()))
    }
    fn sign_ed25519(
        &self,
        input: CdpSigningInput,
        seed: Vec<u8>,
    ) -> Result<Vec<u8>, PortfolioFailure> {
        safe_cex_crypto::ed25519::sign(&input.message, &seed)
            .map(|signature| signature.to_vec())
            .map_err(|message| failure(ExchangeError::new(message)))
    }
    fn sign_ecdsa(
        &self,
        name: &'static str,
        hash: &'static str,
        key: CdpCryptoKey,
        input: CdpSigningInput,
    ) -> CdpAsync<Vec<u8>> {
        if (name, hash) != ("ECDSA", "SHA-256") {
            return Err(failure(ExchangeError::new(
                "Unsupported Coinbase ECDSA algorithm",
            )));
        }
        let key = key
            .downcast_ref::<Es256SigningKey>()
            .ok_or_else(|| failure(ExchangeError::new("Invalid Coinbase CryptoKey handle")))?;
        Ok(Promise::resolved(
            key.sign(&input.message)
                .map(|signature| signature.to_vec())
                .map_err(|message| failure(ExchangeError::new(message))),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cdp_signing::{self, CdpSigningCache, CdpSigningReceiver},
        starbase_auth::CoinbaseStarbasePromise,
    };
    use base64::{Engine, engine::general_purpose::STANDARD};
    use safe_cex_exchange::runtime::{ExchangeFuture, ExchangeTask};
    use serde_json::{Value, json};
    use std::{
        cell::RefCell,
        future::Future,
        io::Write,
        pin::Pin,
        process::{Command, Stdio},
        task::{Context, Poll, Waker},
    };

    #[derive(Default)]
    struct Runtime {
        jobs: RefCell<Vec<Pin<Box<dyn Future<Output = ()>>>>>,
    }
    struct Task;
    impl ExchangeTask for Task {
        fn cancel(&self) {}
    }
    impl ExchangeRuntime for Runtime {
        fn monotonic_now_ms(&self) -> f64 {
            0.0
        }
        fn now_ms(&self) -> f64 {
            1_700_000_000_123.0
        }
        fn sleep(&self, _: u64) -> ExchangeFuture<'static, ()> {
            Box::pin(async { Ok(()) })
        }
        fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
            self.jobs.borrow_mut().push(future);
            Box::new(Task)
        }
        fn console(&self, _: &str, _: &str) {}
        fn unique_id(&self) -> String {
            "fixture-nonce".into()
        }
        fn is_worker(&self) -> bool {
            false
        }
        fn random(&self) -> f64 {
            0.5
        }
        fn report_exception(&self, _: &ExchangeError, _: &Value) {}
        fn report_raw_exception(&self, _: &Value, _: &Value) {}
        fn execute_worker_request(&self, _: Value) -> ExchangeFuture<'static, Value> {
            Box::pin(async { Ok(Value::Null) })
        }
    }
    fn drive<T: Clone>(runtime: &Runtime, promise: CoinbaseStarbasePromise<T>) -> T {
        let mut promise = Box::pin(promise);
        let mut cx = Context::from_waker(Waker::noop());
        for _ in 0..100 {
            if let Poll::Ready(value) = promise.as_mut().poll(&mut cx) {
                return value;
            }
            let jobs = std::mem::take(&mut *runtime.jobs.borrow_mut());
            for mut job in jobs {
                if job.as_mut().poll(&mut cx).is_pending() {
                    runtime.jobs.borrow_mut().push(job);
                }
            }
        }
        panic!("native Coinbase signing promise did not settle");
    }
    fn oracle(input: Value) -> Value {
        let mut process = Command::new("node")
            .arg("../safe-cex-crypto/tests/es256-oracle.cjs")
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        process
            .stdin
            .take()
            .unwrap()
            .write_all(input.to_string().as_bytes())
            .unwrap();
        let output = process.wait_with_output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    }
    #[test]
    fn native_cdp_host_signs_real_jwts_and_reuses_the_imported_key() {
        let source = oracle(json!({}));
        let runtime = Rc::new(Runtime::default());
        let cache = Rc::new(CdpSigningCache::default());
        let r = Rc::new(CdpSigningReceiver {
            host: CryptoHost {
                runtime: runtime.clone(),
            },
            cache: cache.clone(),
            spawn: spawner(runtime.clone()),
        });
        let mut signatures = Vec::new();
        let mut prior_key = None;
        for (label, uri) in [
            ("pkcs8-ws", None),
            (
                "pkcs8-GET",
                Some("GET api.coinbase.com/api/v3/brokerage/portfolios"),
            ),
            (
                "pkcs8-POST",
                Some("POST api.coinbase.com/api/v3/brokerage/orders"),
            ),
        ] {
            let case = source["cases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|c| c["label"] == label)
                .unwrap();
            let bytes: Vec<u8> = serde_json::from_value(case["pkcs8"].clone()).unwrap();
            let secret = format!(
                "-----BEGIN PRIVATE KEY-----\n{}\n-----END PRIVATE KEY-----",
                STANDARD.encode(bytes)
            );
            let before = (safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
                / 1000.0)
                .floor();
            let token = drive(
                runtime.as_ref(),
                cdp_signing::sign(
                    r.clone(),
                    "organizations/fixture/apiKeys/fixture".into(),
                    secret,
                    uri.map(str::to_owned),
                ),
            )
            .unwrap();
            let (payload, signature) = token.rsplit_once('.').unwrap();
            let after = (safe_cex_store::virtual_clock::VIRTUAL_CLOCK.get_current_time_ms()
                / 1000.0)
                .floor();
            let encoding = base64::engine::general_purpose::URL_SAFE_NO_PAD;
            let (header, body) = payload.split_once('.').unwrap();
            let claims: Value = serde_json::from_slice(&encoding.decode(body).unwrap()).unwrap();
            let nbf = claims["nbf"].as_f64().unwrap();
            assert!(
                (before..=after).contains(&nbf),
                "{label} synced clock JWT nbf"
            );
            let expected: Vec<u8> = serde_json::from_value(case["payload"].clone()).unwrap();
            let expected = String::from_utf8(expected).unwrap();
            let (source_header, source_body) = expected.split_once('.').unwrap();
            let mut source_claims: Value =
                serde_json::from_slice(&encoding.decode(source_body).unwrap()).unwrap();
            source_claims["nbf"] = json!(nbf as u64);
            source_claims["exp"] = json!(nbf as u64 + 120);
            let mut source_header: Value =
                serde_json::from_slice(&encoding.decode(source_header).unwrap()).unwrap();
            // The primitive oracle supplies fixture-nonce as its uuid import;
            // the concrete host now uses source uuid with controlled entropy.
            source_header["nonce"] = json!("MMMMMMMMMMMMMMMMMMMMM");
            assert_eq!(
                encoding.decode(header).unwrap(),
                serde_json::to_vec(&source_header).unwrap(),
                "{label} exact source JWT header with native entropy"
            );
            assert_eq!(
                encoding.decode(body).unwrap(),
                serde_json::to_vec(&source_claims).unwrap(),
                "{label} exact source claims with synced time"
            );
            let signature = base64::engine::general_purpose::URL_SAFE_NO_PAD
                .decode(signature)
                .unwrap();
            assert_eq!(signature.len(), 64);
            signatures.push(json!({"payload": payload.as_bytes(), "signature": signature}));
            let key = cache.crypto_key.borrow().clone().unwrap();
            if let Some(prior) = prior_key {
                assert!(
                    Rc::ptr_eq(&key, &prior),
                    "same-secret signing reuses actual imported key"
                );
            }
            prior_key = Some(key);
        }
        assert_eq!(
            oracle(json!({"verify": signatures})),
            json!([
                {"valid": true, "changed": false}, {"valid": true, "changed": false}, {"valid": true, "changed": false}
            ])
        );
    }
    #[test]
    fn native_pem_decode_keeps_the_original_failure_metadata() {
        let host = CryptoHost {
            runtime: Rc::new(Runtime::default()),
        };
        let failure = host.prepare_pkcs8("!").unwrap_err();
        let native = super::super::error::exchange_error(&failure);
        assert_eq!(native.name, "InvalidCharacterError");
        assert!(
            native.origin.is_some(),
            "original codec failure remains the error origin"
        );
    }
}
