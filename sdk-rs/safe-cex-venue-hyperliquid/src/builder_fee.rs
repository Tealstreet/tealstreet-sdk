//! Approval leaf declarations from hyperliquid.builderFee.ts.
//! Browser signing/submission workflows remain pending.
use crate::constants::{TEALSTREET_BUILDER_FEE, builder_fee_rate_from_tenths_bps};
use std::sync::LazyLock;

pub const DEFAULT_BUILDER_FEE_TENTHS_BPS: f64 = TEALSTREET_BUILDER_FEE;
pub static DEFAULT_BUILDER_FEE_RATE: LazyLock<String> =
    LazyLock::new(|| builder_fee_rate_from_tenths_bps(DEFAULT_BUILDER_FEE_TENTHS_BPS));

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ApprovalDomain {
    pub name: &'static str,
    pub version: &'static str,
    pub chain_id: f64,
    pub verifying_contract: &'static str,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ApprovalField {
    pub name: &'static str,
    pub r#type: &'static str,
}

pub const HYPERLIQUID_BUILDER_DOMAIN: ApprovalDomain = ApprovalDomain {
    name: "HyperliquidSignTransaction",
    version: "1",
    chain_id: 42161.0,
    verifying_contract: "0x0000000000000000000000000000000000000000",
};
pub const HYPERLIQUID_BUILDER_DOMAIN_TESTNET: ApprovalDomain = ApprovalDomain {
    name: "HyperliquidSignTransaction",
    version: "1",
    chain_id: 421614.0,
    verifying_contract: "0x0000000000000000000000000000000000000000",
};
pub const APPROVE_BUILDER_FEE_TYPES: &[(&str, &[ApprovalField])] = &[(
    "HyperliquidTransaction:ApproveBuilderFee",
    &[
        ApprovalField {
            name: "hyperliquidChain",
            r#type: "string",
        },
        ApprovalField {
            name: "maxFeeRate",
            r#type: "string",
        },
        ApprovalField {
            name: "builder",
            r#type: "address",
        },
        ApprovalField {
            name: "nonce",
            r#type: "uint64",
        },
    ],
)];
