//! Value-level TransactionClient parameters. Bigints and UTF16 are host-native values,
//! not a JSON serialization contract (JSON.stringify refuses source bigint fields).
use num_bigint::BigInt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOrderKind {
    Limit,
    Market,
    StopLoss,
    StopLossLimit,
    TakeProfit,
    TakeProfitLimit,
}
impl CreateOrderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Limit => "limit",
            Self::Market => "market",
            Self::StopLoss => "stop_loss",
            Self::StopLossLimit => "stop_loss_limit",
            Self::TakeProfit => "take_profit",
            Self::TakeProfitLimit => "take_profit_limit",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreateOrderTimeInForce {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}
impl CreateOrderTimeInForce {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoodTillCancel => "GoodTillCancel",
            Self::ImmediateOrCancel => "ImmediateOrCancel",
            Self::FillOrKill => "FillOrKill",
            Self::PostOnly => "PostOnly",
        }
    }
}
// ts: CreateOrderParams
#[derive(Debug, Clone, PartialEq)]
pub struct CreateOrderParams {
    pub order_id: BigInt,
    pub market_id: f64,
    pub is_buy: bool,
    pub price: BigInt,
    pub amount: BigInt,
    pub post_only: Option<bool>,
    pub reduce_only: Option<bool>,
    pub order_type: Option<CreateOrderKind>,
    pub trigger_price: Option<BigInt>,
    pub order_expiry: Option<BigInt>,
    pub expired_at: Option<BigInt>,
    pub time_in_force: Option<CreateOrderTimeInForce>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
// ts: LighterGroupedOrderType
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LighterGroupedOrderType {
    OneTriggersTheOther = 1,
    OneCancelsTheOther = 2,
    OneTriggersOco = 3,
}
impl LighterGroupedOrderType {
    pub fn value(self) -> f64 {
        self as u8 as f64
    }
}
// ts: CreateGroupedOrdersParams
#[derive(Debug, Clone, PartialEq)]
pub struct CreateGroupedOrdersParams {
    pub grouping_type: LighterGroupedOrderType,
    pub orders: Vec<CreateOrderParams>,
    pub integrator_account_index: Option<BigInt>,
    pub integrator_maker_fee: Option<f64>,
    pub integrator_taker_fee: Option<f64>,
}
// ts: CancelOrderParams
#[derive(Debug, Clone, PartialEq)]
pub struct CancelOrderParams {
    pub order_id: BigInt,
    pub market_id: f64,
}
// ts: TransferParams
#[derive(Debug, Clone, PartialEq)]
pub struct TransferParams {
    pub to_account_index: BigInt,
    pub amount: BigInt,
    pub token_id: Option<f64>,
}
/// The source numeric literal union includes negative zero, which compares equal to 0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarginMode(f64);
impl MarginMode {
    pub fn new(value: f64) -> Option<Self> {
        (value == 0.0 || value == 1.0).then_some(Self(value))
    }
    pub fn value(self) -> f64 {
        self.0
    }
}
// ts: UpdateLeverageParams
#[derive(Debug, Clone, PartialEq)]
pub struct UpdateLeverageParams {
    pub market_id: f64,
    pub leverage: f64,
    pub margin_mode: MarginMode,
}
// ts: ApproveIntegratorParams
#[derive(Debug, Clone, PartialEq)]
pub struct ApproveIntegratorParams {
    pub integrator_account_index: BigInt,
    pub max_perps_taker_fee: f64,
    pub max_perps_maker_fee: f64,
    pub max_spot_taker_fee: f64,
    pub max_spot_maker_fee: f64,
    pub approval_expiry: BigInt,
    pub nonce: Option<BigInt>,
    pub l1_signature: Option<Vec<u16>>,
}
