use crate::zklink_host::{TransactionKind, ZkLinkHandle, ZkLinkHost};
use safe_cex_crypto::zklink::contract::{Contract, ContractSigner};
use safe_cex_exchange::runtime::{ExchangeError, ExchangeFuture};
use safe_cex_num::JsJsonValue as J;
use std::rc::Rc;

pub(super) struct NativeSigner {
    signer: Option<ContractSigner>,
}
fn unavailable() -> ExchangeError {
    ExchangeError::new("ApexOmni native zkLink capability supports contracts only")
}
fn integer(value: &J) -> Result<u32, ExchangeError> {
    let number = super::hosts::js(value)
        .to_number()
        .map_err(super::hosts::error)?;
    // wasm-bindgen's JS -> wasm i32 boundary uses ToInt32; the SDK's Rust
    // u32/u16/u8 arguments then retain their low bits. Rust's saturating `as`
    // alone would corrupt the client's 64-bit-derived slot id.
    Ok(if !number.is_finite() {
        0
    } else {
        number.trunc().rem_euclid(4_294_967_296.) as u32
    })
}
impl NativeSigner {
    pub fn module() -> Self {
        Self { signer: None }
    }
    pub fn from_seed(seed: Option<&str>) -> Result<Self, ExchangeError> {
        let seed = seed.ok_or_else(|| {
            ExchangeError::new("ApexOmni requires an OmniKey seed; wallet provider unavailable")
        })?;
        Ok(Self {
            signer: Some(ContractSigner::from_signature(seed).map_err(ExchangeError::new)?),
        })
    }
}
impl ZkLinkHost for NativeSigner {
    fn init(&self) -> ExchangeFuture<'_, ()> {
        Box::pin(async { Ok(()) })
    }
    fn builder(&self, kind: TransactionKind, args: Vec<J>) -> Result<ZkLinkHandle, ExchangeError> {
        if kind != TransactionKind::Contract || args.len() != 11 {
            return Err(unavailable());
        }
        let text = |index| match &args[index] {
            J::String(v) => Ok(v.clone()),
            _ => Err(ExchangeError::new(
                "zkLink ContractBuilder requires decimal strings",
            )),
        };
        Ok(Rc::new(Contract {
            account_id: integer(&args[0])?,
            sub_account_id: integer(&args[1])? as u8,
            slot_id: integer(&args[2])?,
            nonce: integer(&args[3])?,
            pair_id: integer(&args[4])? as u16,
            size: text(5)?,
            price: text(6)?,
            direction: super::hosts::js(&args[7]).truthy(),
            fee_rates: [integer(&args[8])? as u8, integer(&args[9])? as u8],
            has_subsidy: super::hosts::js(&args[10]).truthy(),
        }))
    }
    fn new_transaction(
        &self,
        kind: TransactionKind,
        builder: ZkLinkHandle,
    ) -> Result<ZkLinkHandle, ExchangeError> {
        if kind != TransactionKind::Contract || !builder.is::<Contract>() {
            return Err(unavailable());
        }
        Ok(builder)
    }
    fn create_signed_contract(&self, tx: ZkLinkHandle) -> Result<J, ExchangeError> {
        let contract = tx.downcast_ref::<Contract>().ok_or_else(unavailable)?;
        self.signer
            .as_ref()
            .ok_or_else(|| ExchangeError::new("zkLink signer is not initialized"))?
            .sign_contract(contract)
            .map(J::from)
            .map_err(ExchangeError::new)
    }
}
