//! Signing-only peer installation and the first closed family codec.
use safe_cex_engine::credential_selection::{CredentialRole, PrimaryReason};
use safe_cex_engine::signing::{SignError, SignReceipt, SignRequest, SignResult};
use safe_cex_engine::{AccountReadScope, CredentialSlot};
use safe_cex_venues::kraken_hosted::{Descriptor, Executor};
use serde::Deserialize;
use wasm_bindgen::prelude::*;

fn error(_: SignError) -> JsError {
    JsError::new("Hosted operation refused")
}
fn parse<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, JsError> {
    if input.len() > 1_048_576 {
        return Err(error(SignError::InvalidRequest));
    }
    serde_json::from_str(input).map_err(|_| error(SignError::InvalidRequest))
}
fn slot(input: &str) -> Result<CredentialSlot, JsError> {
    match input {
        "primary" => Ok(CredentialSlot::Primary),
        "secondary" => Ok(CredentialSlot::Secondary),
        _ => Err(error(SignError::InvalidRequest)),
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Scope {
    Account,
    Balance,
    Orders,
    Positions,
    Settings,
}
impl Scope {
    fn native(self) -> AccountReadScope {
        match self {
            Self::Account => AccountReadScope::Account,
            Self::Balance => AccountReadScope::Balance,
            Self::Orders => AccountReadScope::Orders,
            Self::Positions => AccountReadScope::Positions,
            Self::Settings => AccountReadScope::Settings,
        }
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Reason {
    ClientOrderLookup,
    PostUnknownOrderRead,
    DegradedRead,
    Catalog,
    History,
}
impl Reason {
    fn native(self) -> PrimaryReason {
        match self {
            Self::ClientOrderLookup => PrimaryReason::ClientOrderLookup,
            Self::PostUnknownOrderRead => PrimaryReason::PostUnknownOrderRead,
            Self::DegradedRead => PrimaryReason::DegradedRead,
            Self::Catalog => PrimaryReason::Catalog,
            Self::History => PrimaryReason::History,
        }
    }
}
#[derive(Deserialize)]
#[serde(transparent)]
struct NullableScope(Option<Scope>);
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
enum Role {
    Read {
        scope: NullableScope,
    },
    PrimaryRead {
        reason: Reason,
        scope: NullableScope,
    },
    Mutation,
    SessionAuth,
}
impl Role {
    fn native(self) -> CredentialRole {
        match self {
            Self::Read { scope } => CredentialRole::Read {
                scope: scope.0.map(Scope::native),
            },
            Self::PrimaryRead { scope, reason } => CredentialRole::PrimaryRead {
                scope: scope.0.map(Scope::native),
                reason: reason.native(),
            },
            Self::Mutation => CredentialRole::Mutation,
            Self::SessionAuth => CredentialRole::SessionAuth,
        }
    }
}

#[wasm_bindgen(js_name=krakenHostedDescriptor)]
pub fn describe(input: &str) -> Result<String, JsError> {
    let work: SignRequest = parse(input)?;
    let d = Descriptor::describe(&work).map_err(error)?;
    serde_json::to_string(&d).map_err(|_| error(SignError::InvalidRequest))
}
#[wasm_bindgen(js_name=krakenHostedValidate)]
pub fn validate(
    descriptor: &str,
    receipt: &str,
    selected_slot: &str,
    role: &str,
    now: f64,
    deadline: f64,
) -> Result<(), JsError> {
    let d = Descriptor::from_json(descriptor).map_err(error)?;
    d.validate(
        slot(selected_slot)?,
        parse::<Role>(role)?.native(),
        parse(receipt)?,
        now,
        deadline,
    )
    .map_err(error)
}
#[wasm_bindgen(js_name=krakenHostedVerify)]
pub fn verify(descriptor: &str, result: &str, credential: &str) -> Result<(), JsError> {
    let d = Descriptor::from_json(descriptor).map_err(error)?;
    d.verify(&parse::<SignResult>(result)?, credential)
        .map_err(error)
}

#[wasm_bindgen]
pub struct HostedOperationExecutor {
    inner: Executor,
}
#[wasm_bindgen]
impl HostedOperationExecutor {
    #[wasm_bindgen(constructor)]
    pub fn new(setup: String, spot: bool, testnet: bool) -> Result<Self, JsError> {
        Executor::from_json(setup, spot, testnet)
            .map(|inner| Self { inner })
            .map_err(error)
    }
    pub fn binding(&self, selected_slot: &str) -> Result<String, JsError> {
        let (stable_subject, expected_credential) =
            self.inner.binding(slot(selected_slot)?).map_err(error)?;
        #[derive(serde::Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Binding {
            stable_subject: String,
            expected_credential: String,
        }
        serde_json::to_string(&Binding {
            stable_subject,
            expected_credential,
        })
        .map_err(|_| error(SignError::InvalidResult))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn sign(
        &self,
        descriptor: &str,
        receipt: &str,
        selected_slot: &str,
        role: &str,
        now: f64,
        deadline: f64,
    ) -> Result<String, JsError> {
        let d = Descriptor::from_json(descriptor).map_err(error)?;
        let receipt: SignReceipt = parse(receipt)?;
        let result = self
            .inner
            .execute(
                &d,
                slot(selected_slot)?,
                parse::<Role>(role)?.native(),
                receipt,
                now,
                deadline,
            )
            .map_err(error)?;
        serde_json::to_string(&result).map_err(|_| error(SignError::InvalidResult))
    }
}
