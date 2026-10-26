//! Provider work over immutable GRVT orders. Installed construction is adopted separately.
use crate::account::{BUILDER_MAINNET, BUILDER_TESTNET};
use crate::sign;
use safe_cex_crypto::{ethereum, hex};
use safe_cex_engine::model::TimeInForce;
use safe_cex_engine::signing::{
    Algorithm, Context, Identity, KeyRef, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
};
use safe_cex_engine::{Action, DispatchContext, Method, SignStep};
use serde::Serialize;
use std::sync::{Arc, OnceLock};

const CREATE: &str = "/lite/v1/create_order";
const LOGIN: &str = "/auth/api_key/login";
const ORDER_HEADERS: &[(&str, &str)] = &[("Content-Type", "application/json")];
const LOGIN_HEADERS: &[(&str, &str)] = &[
    ("Content-Type", "application/json"),
    ("X-Grvt-Remember-Me", "true"),
    ("Cookie", "rm=true"),
];

#[derive(Clone, Debug)]
pub struct OrderFields {
    pub subaccount: String,
    pub instrument: String,
    pub asset_id: [u8; 32],
    pub base_decimals: i32,
    pub amount: String,
    pub limit_price: Option<String>,
    pub buying: bool,
    pub time_in_force: TimeInForce,
    pub reduce_only: bool,
    pub nonce: u32,
    pub expiration_ns: i64,
    pub client_id: String,
    pub created_ns: u64,
    pub trigger: Option<Trigger>,
}

/// Materialized once after call admission; preparing a packet never changes these fields.
pub struct FrozenOrder {
    fields: OrderFields,
    digest: [u8; 32],
    testnet: bool,
    time_in_force: &'static str,
    post_only: bool,
}
impl FrozenOrder {
    pub fn new(fields: OrderFields, testnet: bool) -> Result<Self, SignError> {
        let sub_account_id = fields
            .subaccount
            .parse::<u64>()
            .map_err(|_| SignError::InvalidRequest)?;
        let contract_size = sign::scaled_u64(&fields.amount, fields.base_decimals)
            .map_err(|_| SignError::InvalidRequest)?;
        let limit_price = fields
            .limit_price
            .as_deref()
            .map(|price| sign::scaled_u64(price, 9))
            .transpose()
            .map_err(|_| SignError::InvalidRequest)?
            .unwrap_or(0);
        if fields.instrument.is_empty()
            || contract_size == 0
            || (fields.limit_price.is_some() && limit_price == 0)
            || !crate::trading::valid_client(&fields.client_id)
            || fields.expiration_ns <= 0
            || fields.expiration_ns % 1_000_000 != 0
            || fields.created_ns >= fields.expiration_ns as u64
        {
            return Err(SignError::InvalidRequest);
        }
        let (time_in_force, code, post_only) = match fields.time_in_force {
            TimeInForce::GoodTillCancel => ("GOOD_TILL_TIME", 1, false),
            TimeInForce::PostOnly => ("GOOD_TILL_TIME", 1, true),
            TimeInForce::ImmediateOrCancel => ("IMMEDIATE_OR_CANCEL", 3, false),
            TimeInForce::FillOrKill => ("FILL_OR_KILL", 4, false),
        };
        if fields.limit_price.is_none() && post_only {
            return Err(SignError::InvalidRequest);
        }
        let builder = if testnet {
            BUILDER_TESTNET
        } else {
            BUILDER_MAINNET
        };
        let legs = [sign::OrderLeg {
            asset_id: fields.asset_id,
            contract_size,
            limit_price,
            is_buying: fields.buying,
        }];
        let digest = sign::digest(
            &sign::Order {
                sub_account_id,
                is_market: fields.limit_price.is_none(),
                time_in_force: code,
                post_only,
                reduce_only: fields.reduce_only,
                legs: &legs,
                builder: Some(sign::BuilderFee {
                    address: sign::parse_address(builder).map_err(|_| SignError::InvalidRequest)?,
                    fee: sign::builder_fee("0.03").map_err(|_| SignError::InvalidRequest)?,
                }),
                nonce: fields.nonce,
                expiration: fields.expiration_ns,
            },
            if testnet { 326 } else { 325 },
        )
        .map_err(|_| SignError::InvalidRequest)?;
        Ok(Self {
            fields,
            digest,
            testnet,
            time_in_force,
            post_only,
        })
    }
    pub fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
    pub fn expiry_ms(&self) -> f64 {
        (self.fields.expiration_ns / 1_000_000) as f64
    }
    fn render(&self, signer: String, signature: &[u8]) -> Result<String, SignError> {
        let f = &self.fields;
        serde_json::to_string(&Create {
            o: Placement {
                sa: f.subaccount.clone(),
                im: f.limit_price.is_none(),
                ti: self.time_in_force,
                po: self.post_only,
                ro: f.reduce_only,
                l: [Leg {
                    i: f.instrument.clone(),
                    s: f.amount.clone(),
                    lp: f.limit_price.clone(),
                    ib: f.buying,
                }],
                s: Signature {
                    s: signer,
                    r: format!("0x{}", hex(&signature[..32])),
                    s1: format!("0x{}", hex(&signature[32..64])),
                    v: match signature[64] {
                        0 | 1 => signature[64] + 27,
                        value => value,
                    },
                    e: f.expiration_ns.to_string(),
                    n: f.nonce,
                    ci: if self.testnet { "326" } else { "325" },
                },
                m: Metadata {
                    co: f.client_id.clone(),
                    ct: f.created_ns.to_string(),
                    t: f.trigger.clone(),
                },
                b: if self.testnet {
                    BUILDER_TESTNET
                } else {
                    BUILDER_MAINNET
                },
                bf: "0.03",
            },
        })
        .map_err(|_| SignError::InvalidResult)
    }
}

/// One installed account/provider/ref incarnation; recipes share only public proof.
#[derive(Default)]
pub struct SignerBinding {
    address: OnceLock<[u8; 20]>,
}
impl SignerBinding {
    pub fn address(&self) -> Option<String> {
        self.address.get().map(|address| {
            ethereum::get_address(&format!("0x{}", hex(address)))
                .expect("verified twenty-byte address")
        })
    }
    fn seal(&self, address: [u8; 20]) -> Result<(), SignError> {
        if self.address.set(address).is_err() && self.address.get() != Some(&address) {
            return Err(SignError::InvalidResult);
        }
        Ok(())
    }
}

pub struct CreateOrder {
    signing: KeyRef,
    binding: Arc<SignerBinding>,
    order: Arc<FrozenOrder>,
    prepared: Option<SignRequest>,
}
impl CreateOrder {
    pub fn new(
        identity: &Identity,
        binding: Arc<SignerBinding>,
        order: Arc<FrozenOrder>,
    ) -> Result<Self, SignError> {
        let signing = identity.signing_ref()?.clone();
        identity.validate().map_err(|_| SignError::InvalidRequest)?;
        Ok(Self {
            signing,
            binding,
            order,
            prepared: None,
        })
    }
    pub fn url(&self) -> String {
        format!(
            "https://trades{}.grvt.io{CREATE}",
            if self.order.testnet { ".testnet" } else { "" }
        )
    }
}
impl SignStep for CreateOrder {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        ORDER_HEADERS
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        dispatch: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        attest(outbound, &self.url(), ORDER_HEADERS)?;
        if !dispatch.now_ms.is_finite() || dispatch.now_ms < 0. {
            return Err(SignError::InvalidRequest);
        }
        if dispatch.now_ms >= self.order.expiry_ms() {
            return Err(SignError::TimedOut);
        }
        let work = SignRequest {
            receipt: SignReceipt {
                stamp_ms: None,
                expiry_ms: Some(self.order.expiry_ms()),
                nonce: None,
            },
            operations: vec![
                Operation::PublicAddress {
                    key: self.signing.clone(),
                },
                Operation::Sign {
                    algorithm: Algorithm::Secp256k1Digest,
                    key: self.signing.clone(),
                    payload: vec![PayloadPart::Bytes {
                        bytes: self.order.digest.to_vec(),
                    }],
                },
            ],
            context: context(&self.url(), CREATE),
        };
        work.validate()?;
        self.prepared = Some(work.clone());
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let work = self.prepared.take().ok_or(SignError::InvalidResult)?;
        attest(outbound, &self.url(), ORDER_HEADERS)?;
        work.validate_result(result)?;
        let address: [u8; 20] = result.values[0]
            .as_slice()
            .try_into()
            .map_err(|_| SignError::InvalidResult)?;
        let signer = ethereum::get_address(&format!("0x{}", hex(&address)))
            .map_err(|_| SignError::InvalidResult)?;
        if !ethereum::signed_by(&self.order.digest, &result.values[1], &signer) {
            return Err(SignError::InvalidResult);
        }
        let body = self.order.render(signer, &result.values[1])?;
        self.binding.seal(address)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.body = Some(body);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        20_000.
    }
}

/// Nonempty installed login credential only; Empty-intent selection belongs to G3.
pub struct Login {
    credential: KeyRef,
    testnet: bool,
    prepared: Option<SignRequest>,
}
impl Login {
    pub fn new(credential: KeyRef, testnet: bool) -> Self {
        Self {
            credential,
            testnet,
            prepared: None,
        }
    }
    pub fn url(&self) -> String {
        format!(
            "https://edge{}.grvt.io{LOGIN}",
            if self.testnet { ".testnet" } else { "" }
        )
    }
}
impl SignStep for Login {
    fn unsigned_http_headers(&self) -> &'static [(&'static str, &'static str)] {
        LOGIN_HEADERS
    }
    fn prepare(
        &mut self,
        outbound: &mut Action,
        _: &DispatchContext<'_>,
    ) -> Result<SignRequest, SignError> {
        self.prepared = None;
        attest(outbound, &self.url(), LOGIN_HEADERS)?;
        let work = SignRequest {
            receipt: SignReceipt::default(),
            operations: vec![Operation::Present {
                key: self.credential.clone(),
            }],
            context: context(&self.url(), LOGIN),
        };
        work.validate()?;
        self.prepared = Some(work.clone());
        Ok(work)
    }
    fn apply(&mut self, outbound: &mut Action, result: &SignResult) -> Result<(), SignError> {
        let work = self.prepared.take().ok_or(SignError::InvalidResult)?;
        attest(outbound, &self.url(), LOGIN_HEADERS)?;
        work.validate_result(result)?;
        let api_key =
            std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
        #[derive(Serialize)]
        struct Body<'a> {
            api_key: &'a str,
        }
        let body =
            serde_json::to_string(&Body { api_key }).map_err(|_| SignError::InvalidResult)?;
        let Action::Http(http) = outbound else {
            return Err(SignError::InvalidRequest);
        };
        http.body = Some(body);
        Ok(())
    }
    fn freshness_ms(&self) -> f64 {
        20_000.
    }
}
fn context(url: &str, path: &str) -> Context {
    Context {
        venue: "grvt".into(),
        method: "POST".into(),
        host: url
            .strip_prefix("https://")
            .unwrap_or(url)
            .split('/')
            .next()
            .unwrap_or_default()
            .into(),
        path: path.into(),
    }
}
fn attest(outbound: &Action, url: &str, headers: &[(&str, &str)]) -> Result<(), SignError> {
    let Action::Http(http) = outbound else {
        return Err(SignError::InvalidRequest);
    };
    if http.method != Method::Post
        || http.url != url
        || http.body.is_some()
        || !http.signer_headers.is_empty()
        || http.headers.len() != headers.len()
        || !http.headers.iter().zip(headers).all(
            |((name, value), (expected_name, expected_value))| {
                name == expected_name && value == expected_value
            },
        )
    {
        return Err(SignError::InvalidRequest);
    }
    Ok(())
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Create {
    pub(crate) o: Placement,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Placement {
    pub(crate) sa: String,
    pub(crate) im: bool,
    pub(crate) ti: &'static str,
    pub(crate) po: bool,
    pub(crate) ro: bool,
    pub(crate) l: [Leg; 1],
    pub(crate) s: Signature,
    pub(crate) m: Metadata,
    pub(crate) b: &'static str,
    pub(crate) bf: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Leg {
    pub(crate) i: String,
    pub(crate) s: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) lp: Option<String>,
    pub(crate) ib: bool,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Signature {
    pub(crate) s: String,
    pub(crate) r: String,
    pub(crate) s1: String,
    pub(crate) v: u8,
    pub(crate) e: String,
    pub(crate) n: u32,
    pub(crate) ci: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub(crate) struct Metadata {
    pub(crate) co: String,
    pub(crate) ct: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) t: Option<Trigger>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Trigger {
    pub tt: &'static str,
    pub t: TriggerDetails,
}
#[derive(Clone, Debug, Serialize)]
pub struct TriggerDetails {
    pub tb: &'static str,
    pub tp: String,
    pub cp: bool,
    pub is: bool,
}

#[cfg(test)]
#[path = "../tests/support/provider_recipes.rs"]
mod tests;
