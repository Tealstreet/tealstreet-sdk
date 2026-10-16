//! Native http-types transaction values. BigInt and UTF16 fields are not a JSON codec.
use super::transaction_client_types::{LighterGroupedOrderType, MarginMode};
use num_bigint::BigInt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionKind {
    CreateOrder,
    CancelOrder,
    Transfer,
    Withdrawal,
    UpdateLeverage,
    ApproveIntegrator,
    CreateGroupedOrders,
}
impl TransactionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CreateOrder => "create_order",
            Self::CancelOrder => "cancel_order",
            Self::Transfer => "transfer",
            Self::Withdrawal => "withdrawal",
            Self::UpdateLeverage => "update_leverage",
            Self::ApproveIntegrator => "approve_integrator",
            Self::CreateGroupedOrders => "create_grouped_orders",
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub struct TransactionHeader {
    pub data: Vec<u16>,
    pub signature: Vec<u16>,
    pub account_index: BigInt,
    pub api_key_index: f64,
    pub nonce: BigInt,
    pub expired_at: Option<BigInt>,
}
// ts: TxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct TxInfo {
    pub kind: TransactionKind,
    pub header: TransactionHeader,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CreateOrderDetails {
    pub order_id: BigInt,
    pub market_id: f64,
    pub is_buy: bool,
    pub price: BigInt,
    pub amount: BigInt,
    pub post_only: Option<bool>,
    pub reduce_only: Option<bool>,
    pub order_type_value: Option<f64>,
    pub time_in_force_value: Option<f64>,
    pub trigger_price: Option<BigInt>,
    pub order_expiry: Option<BigInt>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
// ts: LighterGroupedOrderDetails
#[derive(Debug, Clone, PartialEq)]
pub struct LighterGroupedOrderDetails {
    pub order_id: BigInt,
    pub market_id: f64,
    pub is_buy: bool,
    pub price: BigInt,
    pub amount: BigInt,
    pub reduce_only: Option<bool>,
    pub order_type_value: Option<f64>,
    pub time_in_force_value: Option<f64>,
    pub trigger_price: Option<BigInt>,
    pub order_expiry: Option<BigInt>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct GroupedOrderDetails {
    pub grouping_type: LighterGroupedOrderType,
    pub orders: Vec<LighterGroupedOrderDetails>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct CancelDetails {
    pub order_id: BigInt,
    pub market_id: f64,
    pub expired_at: Option<BigInt>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct TransferDetails {
    pub to_account_index: BigInt,
    pub amount: BigInt,
    pub token_id: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct LeverageDetails {
    pub market_id: f64,
    pub initial_margin_fraction: f64,
    pub margin_mode: MarginMode,
    pub expired_at: Option<BigInt>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ApproveIntegratorDetails {
    pub integrator_account_index: BigInt,
    pub max_perps_taker_fee: f64,
    pub max_perps_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub approval_expiry: BigInt,
    pub expired_at: Option<BigInt>,
    pub l1_signature: Option<Vec<u16>>,
}
// ts: L2CreateOrderTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2CreateOrderTxInfo {
    pub header: TransactionHeader,
    pub order_details: CreateOrderDetails,
}
// ts: L2CreateGroupedOrdersTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2CreateGroupedOrdersTxInfo {
    pub header: TransactionHeader,
    pub grouped_order_details: GroupedOrderDetails,
}
// ts: L2CancelOrderTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2CancelOrderTxInfo {
    pub header: TransactionHeader,
    pub cancel_details: CancelDetails,
}
// ts: L2TransferTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2TransferTxInfo {
    pub header: TransactionHeader,
    pub transfer_details: TransferDetails,
}
// ts: L2UpdateLeverageTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2UpdateLeverageTxInfo {
    pub header: TransactionHeader,
    pub leverage_details: LeverageDetails,
}
// ts: L2ApproveIntegratorTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2ApproveIntegratorTxInfo {
    pub header: TransactionHeader,
    pub approve_integrator_details: ApproveIntegratorDetails,
}
// ts: L2WithdrawalTxInfo
#[derive(Debug, Clone, PartialEq)]
pub struct L2WithdrawalTxInfo {
    pub header: TransactionHeader,
}
pub trait TransactionInfo {
    fn kind(&self) -> TransactionKind;
    fn header(&self) -> &TransactionHeader;
}
impl TransactionInfo for L2CreateOrderTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::CreateOrder
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2CreateGroupedOrdersTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::CreateGroupedOrders
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2CancelOrderTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::CancelOrder
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2TransferTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::Transfer
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2UpdateLeverageTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::UpdateLeverage
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2ApproveIntegratorTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::ApproveIntegrator
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for L2WithdrawalTxInfo {
    fn kind(&self) -> TransactionKind {
        TransactionKind::Withdrawal
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
impl TransactionInfo for TxInfo {
    fn kind(&self) -> TransactionKind {
        self.kind
    }
    fn header(&self) -> &TransactionHeader {
        &self.header
    }
}
// ts: AnyTxInfo
#[derive(Debug, Clone, PartialEq)]
pub enum AnyTxInfo {
    CreateOrder(L2CreateOrderTxInfo),
    CreateGroupedOrders(L2CreateGroupedOrdersTxInfo),
    CancelOrder(L2CancelOrderTxInfo),
    Transfer(L2TransferTxInfo),
    UpdateLeverage(L2UpdateLeverageTxInfo),
    ApproveIntegrator(L2ApproveIntegratorTxInfo),
    Withdrawal(L2WithdrawalTxInfo),
}
impl TransactionInfo for AnyTxInfo {
    fn kind(&self) -> TransactionKind {
        match self {
            Self::CreateOrder(v) => v.kind(),
            Self::CreateGroupedOrders(v) => v.kind(),
            Self::CancelOrder(v) => v.kind(),
            Self::Transfer(v) => v.kind(),
            Self::UpdateLeverage(v) => v.kind(),
            Self::ApproveIntegrator(v) => v.kind(),
            Self::Withdrawal(v) => v.kind(),
        }
    }
    fn header(&self) -> &TransactionHeader {
        match self {
            Self::CreateOrder(v) => v.header(),
            Self::CreateGroupedOrders(v) => v.header(),
            Self::CancelOrder(v) => v.header(),
            Self::Transfer(v) => v.header(),
            Self::UpdateLeverage(v) => v.header(),
            Self::ApproveIntegrator(v) => v.header(),
            Self::Withdrawal(v) => v.header(),
        }
    }
}
