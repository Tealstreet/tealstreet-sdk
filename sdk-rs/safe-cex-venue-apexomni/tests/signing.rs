use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::{JsJsonValue as J, numeric_ingress::js_from_str};
use safe_cex_venue_apexomni::{transaction_records::*, transaction_signing::*, zklink_host::*};
use serde_json::Value;
use std::{cell::RefCell, process::Command, rc::Rc, sync::OnceLock};
#[path = "support/native_values.rs"]
pub mod native;

fn corpus() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/signing.cjs"))
            .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "signing oracle: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[derive(Default)]
struct Host {
    operations: RefCell<Vec<J>>,
    wrapper_signature: bool,
}
struct Built {
    kind: TransactionKind,
}
impl Host {
    fn record(&self, name: &str, mut args: Vec<J>) {
        args.insert(0, J::String(name.to_owned()));
        self.operations.borrow_mut().push(J::Array(args));
    }
    fn signed(&self) -> J {
        let mut fields = vec![
            ("nativeSignature".into(), J::String("signature-é-😀".into())),
            ("nonce".into(), J::Number(97.25)),
        ];
        if self.wrapper_signature {
            fields.push((
                "signature".into(),
                J::Object(vec![(
                    "signature".into(),
                    J::String("wrapper-signature-é-😀".into()),
                )]),
            ));
        }
        J::Object(fields)
    }
    fn assert_handle(&self, tx: &ZkLinkHandle, kind: TransactionKind) {
        assert_eq!(
            tx.downcast_ref::<Built>().unwrap().kind,
            kind,
            "source opaque transaction handle"
        );
    }
}
impl ZkLinkHost for Host {
    fn init(&self) -> ExchangeFuture<'_, ()> {
        self.record("init", vec![]);
        Box::pin(async { Ok(()) })
    }
    fn builder(&self, kind: TransactionKind, args: Vec<J>) -> Result<ZkLinkHandle, ExchangeError> {
        self.record(&format!("builder:{kind:?}"), args);
        Ok(Rc::new(Built { kind }))
    }
    fn new_transaction(
        &self,
        kind: TransactionKind,
        tx: ZkLinkHandle,
    ) -> Result<ZkLinkHandle, ExchangeError> {
        self.assert_handle(&tx, kind);
        self.record(&format!("new:{kind:?}"), vec![]);
        Ok(tx)
    }
    fn create_signed_contract(&self, tx: ZkLinkHandle) -> Result<J, ExchangeError> {
        self.assert_handle(&tx, TransactionKind::Contract);
        self.record("sign:Contract", vec![]);
        Ok(self.signed())
    }
}
fn run(name: &str) {
    let all = corpus()["rows"].as_array().unwrap();
    assert_eq!(all.len(), 6, "signing total floor");
    let mut count = 0;
    for row in all.iter().filter(|r| r["name"] == name) {
        let host = Host::default();
        let raw = row["raw"].as_str().unwrap();
        let result = futures::executor::block_on(async {
            match name {
                "signContract" => {
                    sign_contract(&host, &js_from_str::<SignContractPayload>(raw).unwrap()).await
                }
                _ => panic!(),
            }
        });
        if row["result"]["ok"] == true {
            native::check_fields(
                &result.unwrap(),
                &row["result"]["value"],
                &format!("{name} result"),
            );
        } else {
            assert_eq!(
                result.unwrap_err().message,
                row["result"]["message"].as_str().unwrap(),
                "{name} native error"
            );
        }
        native::check(
            &J::Array(host.operations.into_inner()),
            &row["operations"],
            &format!("{name} operations"),
        );
        count += 1;
    }
    assert_eq!(count, 6, "{name} independent native case floor");
}
#[test]
fn contract_native() {
    run("signContract");
}
#[test]
fn initialization_native() {
    let host = Host::default();
    let sdk = SdkInitialization::default();
    futures::executor::block_on(async {
        sdk.init_sdk(&host).await.unwrap();
        sdk.init_sdk(&host).await.unwrap();
    });
    native::check(
        &J::Array(host.operations.into_inner()),
        &corpus()["initialization"],
        "initSdk operations",
    );
}

#[test]
fn exchange_sign_order_native() {
    let rows = corpus()["wrapperRows"].as_array().unwrap();
    assert_eq!(rows.len(), 7, "exchange signOrder source floor");
    let mut compared = 0;
    for (index, row) in rows.iter().enumerate() {
        let host = Host {
            wrapper_signature: index < 6,
            ..Default::default()
        };
        let payload = js_from_str::<SignContractPayload>(row["raw"].as_str().unwrap()).unwrap();
        let result = futures::executor::block_on(
            safe_cex_venue_apexomni::exchange::ApexOmniExchange::sign_order(
                &host,
                &payload,
                &J::String("ignored-prefix".into()),
            ),
        );
        assert_eq!(
            result.is_ok(),
            row["result"]["ok"].as_bool().unwrap(),
            "exchange signOrder native result kind"
        );
        if row["result"]["ok"] == true {
            native::check(
                &result.unwrap(),
                &row["result"]["value"],
                "exchange signOrder native result",
            );
        } else {
            let error = result.unwrap_err();
            assert_eq!(
                error.message,
                row["result"]["message"].as_str().unwrap(),
                "exchange signOrder native error cause"
            );
            assert_eq!(
                error.name,
                row["result"]["name"].as_str().unwrap(),
                "exchange signOrder native error name"
            );
        }
        native::check(
            &J::Array(host.operations.into_inner()),
            &row["operations"],
            "exchange signOrder native operation arguments",
        );
        compared += 1;
    }
    assert_eq!(compared, 7, "exchange signOrder comparison floor");
}
