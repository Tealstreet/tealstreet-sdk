use serde::Deserialize;
use serde_json::value::RawValue;
use std::borrow::Cow;

pub use safe_cex_engine::de::Number as Decimal;
use safe_cex_engine::de::optional_number_or_blank;
#[derive(Clone, Deserialize)]
pub struct Portfolio {
    pub uuid: String,
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub deleted: bool,
}
#[derive(Deserialize)]
pub struct Portfolios {
    pub portfolios: Vec<Portfolio>,
}
#[derive(Deserialize)]
pub struct Money {
    pub value: Decimal,
}
#[derive(Deserialize)]
pub struct IntxBalance {
    pub summary: IntxSummary,
}
#[derive(Deserialize)]
pub struct IntxSummary {
    pub total_balance: Money,
    pub buying_power: Money,
}
#[derive(Deserialize)]
pub struct CfmBalance {
    pub balance_summary: CfmSummary,
}
#[derive(Deserialize)]
pub struct CfmSummary {
    pub total_usd_balance: Money,
    pub available_margin: Money,
    #[serde(default)]
    pub unrealized_pnl: Option<Money>,
}
#[derive(Deserialize)]
pub struct Breakdown {
    pub breakdown: BreakdownBody,
}
#[derive(Deserialize)]
pub struct BreakdownBody {
    pub spot_positions: Vec<SpotBalance>,
    pub portfolio_balances: Option<PortfolioBalances>,
}
#[derive(Deserialize)]
pub struct PortfolioBalances {
    pub total_cash_equivalent_balance: Option<CashEquivalent>,
}
#[derive(Deserialize)]
pub struct CashEquivalent {
    pub value: Decimal,
    pub currency: String,
}
impl BreakdownBody {
    pub fn cash_balance(&self) -> Result<safe_cex_engine::model::Balance, String> {
        let (mut row_total, mut free) = (0., 0.);
        for row in self
            .spot_positions
            .iter()
            .filter(|row| matches!(row.asset.as_str(), "USD" | "USDC"))
        {
            let total = row
                .total_balance_fiat
                .map_or(row.total_balance_crypto.0, |n| n.0);
            let available = row
                .available_to_trade_fiat
                .map_or(row.available_to_trade_crypto.0, |n| n.0);
            if total < 0.
                || available < 0.
                || available > total
                || row.total_balance_crypto.0 < 0.
                || row.available_to_trade_crypto.0 < 0.
                || row.available_to_trade_crypto.0 > row.total_balance_crypto.0
            {
                return Err("Coinbase invalid spot cash balance".into());
            }
            row_total += total;
            free += available;
        }
        let total = match self
            .portfolio_balances
            .as_ref()
            .and_then(|p| p.total_cash_equivalent_balance.as_ref())
        {
            Some(cash) if cash.currency == "USD" => cash.value.0,
            Some(_) => return Err("Coinbase spot cash balance is not USD".into()),
            None => row_total,
        };
        if !total.is_finite() || !free.is_finite() || total < 0. || free < 0. || free > total {
            return Err("Coinbase invalid spot cash balance".into());
        }
        Ok(safe_cex_engine::model::Balance {
            total,
            free,
            used: total - free,
            upnl: 0.,
        })
    }
}
#[derive(Deserialize)]
pub struct SpotBalance {
    pub asset: String,
    #[serde(deserialize_with = "spot_total_crypto")]
    pub total_balance_crypto: Decimal,
    #[serde(deserialize_with = "spot_available_crypto")]
    pub available_to_trade_crypto: Decimal,
    #[serde(default, deserialize_with = "spot_total_fiat")]
    pub total_balance_fiat: Option<Decimal>,
    #[serde(default, deserialize_with = "spot_available_fiat")]
    pub available_to_trade_fiat: Option<Decimal>,
}
macro_rules! spot_amount {
    ($name:ident, $ty:ty, $field:literal) => {
        fn $name<'de, D: serde::Deserializer<'de>>(d: D) -> Result<$ty, D::Error> {
            <$ty>::deserialize(d).map_err(|_| {
                serde::de::Error::custom(concat!(
                    "Coinbase invalid field `spot_positions.",
                    $field,
                    "`"
                ))
            })
        }
    };
}
spot_amount!(spot_total_crypto, Decimal, "total_balance_crypto");
spot_amount!(spot_available_crypto, Decimal, "available_to_trade_crypto");
spot_amount!(spot_total_fiat, Option<Decimal>, "total_balance_fiat");
spot_amount!(
    spot_available_fiat,
    Option<Decimal>,
    "available_to_trade_fiat"
);
#[derive(Deserialize)]
pub struct IntxPosition<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    pub entry_vwap: Money,
    pub mark_price: Money,
    pub liquidation_price: Money,
    #[serde(borrow)]
    pub position_side: Cow<'a, str>,
    pub net_size: Decimal,
    #[serde(default)]
    pub leverage: Option<Decimal>,
}
#[derive(Deserialize)]
pub struct CfmPosition<'a> {
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    pub number_of_contracts: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
}
#[derive(Default, Deserialize)]
pub struct Page<'a> {
    #[serde(default)]
    pub has_next: bool,
    #[serde(default, borrow)]
    pub cursor: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    pub client_order_id: Cow<'a, str>,
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(default, borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, borrow)]
    pub order_side: Cow<'a, str>,
    #[serde(default, borrow)]
    pub order_type: Cow<'a, str>,
    #[serde(default)]
    pub filled_size: Option<Decimal>,
    #[serde(default)]
    pub cumulative_quantity: Option<Decimal>,
    #[serde(default)]
    pub leaves_quantity: Option<Decimal>,
    #[serde(default, borrow)]
    pub limit_price: Cow<'a, str>,
    #[serde(default, borrow)]
    pub stop_price: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number_or_blank")]
    pub average_filled_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number_or_blank")]
    pub avg_price: Option<f64>,
    #[serde(default, borrow)]
    pub order_configuration: Option<Configuration<'a>>,
    #[serde(default)]
    pub reduce_only: Option<bool>,
}
#[derive(Deserialize)]
pub struct Configuration<'a> {
    #[serde(default, borrow)]
    pub market_market_ioc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub market_market_fok: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_gtc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_gtd: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub limit_limit_fok: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub sor_limit_ioc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub stop_limit_stop_limit_gtc: Option<Size<'a>>,
    #[serde(default, borrow)]
    pub stop_limit_stop_limit_gtd: Option<Size<'a>>,
}
#[derive(Deserialize)]
pub struct Size<'a> {
    #[serde(default, borrow)]
    pub base_size: Cow<'a, str>,
    #[serde(default, borrow)]
    pub quote_size: Cow<'a, str>,
    #[serde(default, borrow)]
    pub limit_price: Cow<'a, str>,
    #[serde(default, borrow)]
    pub stop_price: Cow<'a, str>,
    #[serde(default)]
    pub post_only: bool,
}
#[derive(Deserialize)]
pub struct UserFrame<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    pub sequence_num: Option<u64>,
    #[serde(default, borrow)]
    pub events: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct UserEvent<'a> {
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(borrow)]
    pub orders: Option<Vec<&'a RawValue>>,
}
#[derive(Deserialize)]
pub struct EditSource<'a> {
    #[serde(default, borrow)]
    pub filled_size: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub cumulative_quantity: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub leaves_quantity: Option<Cow<'a, str>>,
    #[serde(default, borrow)]
    pub attached_order_id: Cow<'a, str>,
    #[serde(default, borrow)]
    pub attached_order_configuration: Option<&'a RawValue>,
}

#[derive(Deserialize)]
pub struct OrderDetail<'a> {
    #[serde(borrow)]
    pub order: OrderSizing<'a>,
}
#[derive(Deserialize)]
pub struct OrderSizing<'a> {
    #[serde(borrow)]
    pub order_id: Cow<'a, str>,
    #[serde(borrow)]
    pub product_id: Cow<'a, str>,
    #[serde(borrow)]
    pub order_configuration: Configuration<'a>,
}
#[derive(Deserialize)]
pub struct KeyPermissions {
    pub portfolio_uuid: String,
}
