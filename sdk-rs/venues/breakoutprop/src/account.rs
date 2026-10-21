//! One portfolio response is one reconciliation transaction.
use crate::Pending;
use safe_cex_engine::Level;
use safe_cex_engine::log::code;
use safe_cex_engine::model::{
    Balance, MarginType, Order, OrderSide, OrderStatus, OrderType, Position, PositionMode,
    PositionSide, TimeInForce,
};
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{Cx, Store};
use serde::Deserialize;
use std::borrow::Cow;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Portfolio<'a> {
    #[serde(borrow)]
    account: Cow<'a, str>,
    version: u64,
    #[serde(borrow)]
    balances: Vec<Cash<'a>>,
    #[serde(borrow)]
    positions: Vec<RawPosition<'a>>,
    #[serde(borrow)]
    orders: Vec<RawOrder<'a>>,
}
#[derive(Deserialize)]
struct Cash<'a> {
    #[serde(borrow)]
    account: Cow<'a, str>,
    #[serde(borrow)]
    currency: Cow<'a, str>,
    value: f64,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawPosition<'a> {
    #[serde(borrow)]
    account: Cow<'a, str>,
    #[serde(borrow)]
    symbol: Cow<'a, str>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(borrow)]
    position_code: Cow<'a, str>,
    quantity: f64,
    quantity_notional: f64,
    open_price: f64,
    margin_rate: f64,
    #[serde(borrow)]
    last_update_time: Cow<'a, str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawOrder<'a> {
    #[serde(borrow)]
    account: Cow<'a, str>,
    #[serde(borrow)]
    order_code: Cow<'a, str>,
    #[serde(default, borrow)]
    client_order_id: Option<Cow<'a, str>>,
    #[serde(borrow)]
    instrument: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    kind: Cow<'a, str>,
    #[serde(borrow)]
    status: Cow<'a, str>,
    final_status: bool,
    #[serde(borrow)]
    transaction_time: Cow<'a, str>,
    limit_price: Option<f64>,
    stop_price: Option<f64>,
    #[serde(borrow)]
    legs: Vec<Leg<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Leg<'a> {
    #[serde(default, borrow)]
    instrument: Option<Cow<'a, str>>,
    #[serde(borrow)]
    side: Cow<'a, str>,
    #[serde(borrow)]
    position_effect: Cow<'a, str>,
    price: Option<f64>,
    quantity: f64,
    filled_quantity: f64,
    remaining_quantity: f64,
}

pub(crate) struct Snapshot {
    pub version: u64,
    balance: Balance,
    positions: Vec<Position>,
    pub(crate) orders: Vec<Order>,
    settings: Vec<(String, MarketSetting)>,
    /// Rows this engine cannot model, e.g. off-catalogue instruments or
    /// terminal-placed multi-leg orders; logged instead of failing the snapshot.
    skipped: Vec<String>,
}
impl Snapshot {
    pub(crate) fn commit(self, cx: &mut Cx<'_, Pending>) {
        for row in &self.skipped {
            cx.notice(
                Level::Warn,
                code::ROW_SKIPPED,
                format!("BreakoutProp skipped {row}"),
            );
        }
        cx.store.set_balance(self.balance);
        cx.store.retain_position_ids();
        cx.store.set_positions(self.positions);
        cx.store.set_orders(self.orders);
        cx.orders_reconciled(None);
        let settings = self
            .settings
            .into_iter()
            .filter_map(|(symbol, setting)| cx.store.market_index(&symbol).map(|id| (id, setting)))
            .collect::<Vec<_>>();
        cx.store.set_settings(settings);
    }
}

pub(crate) fn parse_portfolio(
    item: &str,
    store: &Store,
    account: &str,
) -> Result<Option<Snapshot>, String> {
    let raw: Portfolio<'_> = serde_json::from_str(item).map_err(|_| "Invalid DXtrade portfolio")?;
    if raw.account != account {
        return Ok(None);
    }
    let mut positions = Vec::with_capacity(raw.positions.len());
    let mut settings = store
        .markets()
        .iter()
        .map(|m| {
            (
                m.symbol.clone(),
                MarketSetting {
                    leverage: 0.0,
                    margin_type: Some(MarginType::Cross),
                    ..MarketSetting::default()
                },
            )
        })
        .collect::<Vec<_>>();
    let mut balance = Balance::default();
    let mut cash = None;
    for row in raw.balances {
        if row.account != account {
            return Err("Portfolio balance account mismatch".into());
        }
        finite(row.value)?;
        if row.currency == "USD" && cash.replace(row.value).is_some() {
            return Err("Duplicate USD portfolio balance".into());
        }
    }
    let mut skipped = Vec::new();
    let mut position_ids = std::collections::HashSet::new();
    for p in raw.positions {
        if p.account != account {
            return Err("Portfolio position account mismatch".into());
        }
        for value in [p.quantity, p.quantity_notional, p.open_price, p.margin_rate] {
            finite(value)?;
        }
        if p.position_code.is_empty()
            || !position_ids.insert(p.position_code.to_string())
            || p.quantity < 0.0
            || p.open_price <= 0.0
            || p.margin_rate <= 0.0
        {
            return Err("Invalid DXtrade position".into());
        }
        let side = match p.side.as_ref() {
            "BUY" => PositionSide::Long,
            "SELL" => PositionSide::Short,
            _ => return Err("Invalid DXtrade position side".into()),
        };
        let mark = store
            .ticker(&p.symbol)
            .map(|t| if t.mark != 0.0 { t.mark } else { t.last })
            .filter(|p| p.is_finite() && *p > 0.0)
            .unwrap_or(p.open_price);
        let notional = p.quantity_notional.abs();
        let margin = notional * p.margin_rate;
        let upnl = if side == PositionSide::Long {
            mark - p.open_price
        } else {
            p.open_price - mark
        } * p.quantity;
        let leverage = 1.0 / p.margin_rate;
        for value in [margin, upnl, leverage] {
            finite(value)?;
        }
        balance.used += margin;
        balance.upnl += upnl;
        // Margin still counts against the account; the position itself is not tradable here.
        if store.market(&p.symbol).is_none() {
            skipped.push(format!(
                "position {} on unlisted {}",
                p.position_code, p.symbol
            ));
            continue;
        }
        if let Some((_, setting)) = settings.iter_mut().find(|(symbol, _)| symbol == &p.symbol) {
            setting.leverage = leverage;
        }
        if p.quantity == 0.0 {
            continue;
        }
        positions.push(Position {
            id: p.position_code.into_owned(),
            symbol: p.symbol.into_owned(),
            side,
            entry_price: p.open_price,
            notional,
            leverage,
            margin_type: Some(MarginType::Cross),
            position_mode: Some(PositionMode::Oneway),
            unrealized_pnl: upnl,
            realized_pnl: None,
            contracts: p.quantity,
            liquidation_price: 0.0,
            margin_size: Some(margin),
            last_update: Some(crate::wire::timestamp(&p.last_update_time)?),
        });
    }
    balance.total = cash.unwrap_or(0.0) + balance.upnl;
    balance.free = balance.total - balance.used;
    for value in [balance.total, balance.free, balance.used, balance.upnl] {
        finite(value)?;
    }
    let mut orders = Vec::with_capacity(raw.orders.len());
    let mut ids = std::collections::HashSet::new();
    for order in raw.orders {
        if order.account != account {
            return Err("Portfolio order account mismatch".into());
        }
        if order.final_status {
            continue;
        }
        if order.order_code.is_empty() || !ids.insert(order.order_code.to_string()) {
            return Err("Invalid DXtrade open order".into());
        }
        if !matches!(order.status.as_ref(), "WORKING" | "ACCEPTED" | "IN_REVIEW")
            || order.legs.len() != 1
            || store.market(&order.instrument).is_none()
            || !matches!(order.kind.as_ref(), "LIMIT" | "MARKET" | "STOP")
        {
            skipped.push(format!(
                "order {} ({} {} with {} legs on {})",
                order.order_code,
                order.status,
                order.kind,
                order.legs.len(),
                order.instrument
            ));
            continue;
        }
        let leg = &order.legs[0];
        if leg
            .instrument
            .as_deref()
            .is_some_and(|symbol| symbol != order.instrument)
        {
            return Err("Portfolio order leg instrument mismatch".into());
        }
        for value in [leg.quantity, leg.filled_quantity, leg.remaining_quantity] {
            finite(value)?;
        }
        let tolerance = (leg.quantity.abs() * 1e-10).max(1e-12);
        if (leg.filled_quantity + leg.remaining_quantity - leg.quantity).abs() > tolerance
            || leg.filled_quantity > leg.quantity + tolerance
            || leg.remaining_quantity > leg.quantity + tolerance
            || leg.quantity < 0.0
            || (leg.quantity == 0.0
                && (leg.position_effect != "CLOSE"
                    || leg.filled_quantity != 0.0
                    || leg.remaining_quantity != 0.0))
            || leg.filled_quantity < 0.0
            || leg.remaining_quantity < 0.0
        {
            return Err("Invalid DXtrade order quantities".into());
        }
        // DXtrade documents prices on Order; the legacy host also sends leg.price.
        let (kind, price) = match order.kind.as_ref() {
            "LIMIT" => (OrderType::Limit, order.limit_price.or(leg.price)),
            "MARKET" => (OrderType::Market, Some(leg.price.unwrap_or(0.0))),
            "STOP" => (OrderType::StopLoss, order.stop_price.or(leg.price)),
            _ => return Err("Unsupported DXtrade order type".into()),
        };
        let price = price.ok_or("Missing DXtrade order price")?;
        finite(price)?;
        if price < 0.0 || (kind != OrderType::Market && price == 0.0) {
            return Err("Invalid DXtrade order price".into());
        }
        let side = match leg.side.as_ref() {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return Err("Invalid DXtrade order side".into()),
        };
        let reduce_only = match leg.position_effect.as_ref() {
            "CLOSE" => true,
            "OPEN" => false,
            _ => return Err("Invalid DXtrade position effect".into()),
        };
        orders.push(Order {
            id: order.order_code.into_owned(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: order.instrument.into_owned(),
            kind,
            side,
            price,
            amount: leg.quantity,
            filled: leg.filled_quantity,
            remaining: leg.remaining_quantity,
            reduce_only,
            close: None,
            client_order_id: order
                .client_order_id
                .filter(|id| !id.is_empty())
                .map(Cow::into_owned),
            time_in_force: Some(TimeInForce::GoodTillCancel),
            last_update: crate::wire::timestamp(&order.transaction_time)?,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        });
    }
    Ok(Some(Snapshot {
        version: raw.version,
        balance,
        positions,
        orders,
        settings,
        skipped,
    }))
}
fn finite(value: f64) -> Result<(), String> {
    if value.is_finite() {
        Ok(())
    } else {
        Err("Invalid DXtrade portfolio number".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::model::{Limits, Market, MinMax, Precision, Ticker};
    fn store() -> Store {
        let mut store = Store::default();
        store.set_markets(vec![Market {
            id: "ETHUSD".into(),
            symbol: "ETHUSD".into(),
            pretty_symbol: None,
            base: "ETH".into(),
            quote: "USD".into(),
            active: true,
            precision: Precision {
                amount: 0.01,
                price: 0.01,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.01,
                    max: 100.0,
                },
                leverage: MinMax { min: 1.0, max: 5.0 },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: Some(false),
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: None,
            is_read_only: Some(false),
            taker_fee: Some(0.0),
            maker_fee: Some(0.0),
        }]);
        store
    }
    const POSITION: &str = r#"{"account":"default:00002","symbol":"ETHUSD","side":"BUY","positionCode":"76515503","quantity":0.01,"quantityNotional":0.01,"openPrice":3656.01,"marginRate":0.2,"lastUpdateTime":"2024-12-11T06:00:49.087Z"}"#;
    const ORDER: &str = r#"{"account":"default:00002","orderCode":"4471153:55639","instrument":"ETHUSD","type":"LIMIT","status":"WORKING","finalStatus":false,"transactionTime":"2024-12-11T05:55:49.900Z","legs":[{"side":"BUY","positionEffect":"CLOSE","price":3529.47,"quantity":0.01,"filledQuantity":0,"remainingQuantity":0.01}]}"#;
    fn payload(positions: &str, orders: &str) -> String {
        format!(
            r#"{{"account":"default:00002","version":86,"balances":[{{"account":"default:00002","currency":"USD","value":4999.96}}],"positions":[{positions}],"orders":[{orders}]}}"#
        )
    }
    #[test]
    fn documented_portfolio_keeps_native_notional_and_every_position_row() {
        let snapshot = parse_portfolio(
            &payload(
                &format!("{POSITION},{}", POSITION.replace("76515503", "76515542")),
                ORDER,
            ),
            &store(),
            "default:00002",
        )
        .unwrap()
        .unwrap();
        assert_eq!(snapshot.positions.len(), 2);
        assert_eq!(snapshot.positions[0].notional, 0.01);
        assert_eq!(snapshot.positions[0].margin_size, Some(0.002));
        assert_eq!(snapshot.positions[0].last_update, Some(1_733_896_849_087.0));
        assert_eq!(snapshot.balance.used, 0.004);
        assert_eq!(snapshot.balance.upnl, 0.0);
        assert_eq!(snapshot.balance.total, 4999.96);
        assert!(snapshot.orders[0].reduce_only);
        assert_eq!(snapshot.orders[0].last_update, 1_733_896_549_900.0);
    }
    #[test]
    fn short_pnl_uses_previous_stored_mark_and_cash_plus_estimates() {
        let mut store = store();
        store.set_tickers(vec![Ticker {
            id: "ETHUSD".into(),
            symbol: "ETHUSD".into(),
            mark: 3600.0,
            ..Ticker::default()
        }]);
        let snapshot = parse_portfolio(
            &payload(&POSITION.replace("BUY", "SELL"), ""),
            &store,
            "default:00002",
        )
        .unwrap()
        .unwrap();
        let expected = (3656.01 - 3600.0) * 0.01;
        assert!((snapshot.balance.upnl - expected).abs() < 1e-10);
        assert!((snapshot.balance.total - (4999.96 + expected)).abs() < 1e-10);
        assert_eq!(snapshot.positions[0].side, PositionSide::Short);
    }
    #[test]
    fn malformed_order_discards_entire_transaction() {
        let snapshot = parse_portfolio(
            &payload(POSITION, &ORDER.replace("CLOSE", "INVALID")),
            &store(),
            "default:00002",
        );
        assert!(snapshot.is_err());
    }
    #[test]
    fn selected_account_is_required() {
        assert!(
            parse_portfolio(&payload(POSITION, ORDER), &store(), "default:12")
                .unwrap()
                .is_none()
        );
    }
    #[test]
    fn malformed_timestamp_and_zero_margin_fail_closed() {
        for position in [
            POSITION.replace("0.2", "0"),
            POSITION.replace("2024-12-11T06:00:49.087Z", "yesterday"),
        ] {
            assert!(
                parse_portfolio(&payload(&position, ORDER), &store(), "default:00002").is_err()
            );
        }
    }
    #[test]
    fn successful_empty_snapshot_clears_exposure() {
        let snapshot = parse_portfolio(&payload("", ""), &store(), "default:00002")
            .unwrap()
            .unwrap();
        assert!(snapshot.positions.is_empty());
        assert!(snapshot.orders.is_empty());
        assert_eq!(snapshot.settings[0].1.leverage, 0.0);
        assert_eq!(snapshot.balance.total, 4999.96);
    }
    #[test]
    fn duplicated_order_identity_is_not_committed() {
        assert!(
            parse_portfolio(
                &payload("", &format!("{ORDER},{ORDER}")),
                &store(),
                "default:00002"
            )
            .is_err()
        );
    }
    #[test]
    fn unmodelled_rows_are_skipped_without_discarding_the_snapshot() {
        let xbt = POSITION
            .replace("ETHUSD", "XBTUSD")
            .replace("76515503", "90000001");
        let multi_leg = ORDER.replace("4471153:55639", "multi").replace(
            r#""legs":[{"#,
            r#""legs":[{"side":"SELL","positionEffect":"OPEN","price":1,"quantity":1,"filledQuantity":0,"remainingQuantity":1},{"#,
        );
        let trailing = ORDER
            .replace("4471153:55639", "trail")
            .replace(r#""type":"LIMIT""#, r#""type":"TRAILING_STOP""#);
        let snapshot = parse_portfolio(
            &payload(
                &format!("{POSITION},{xbt}"),
                &format!("{ORDER},{multi_leg},{trailing}"),
            ),
            &store(),
            "default:00002",
        )
        .unwrap()
        .unwrap();
        assert_eq!(snapshot.positions.len(), 1);
        assert_eq!(snapshot.positions[0].symbol, "ETHUSD");
        assert_eq!(snapshot.balance.used, 0.004);
        assert_eq!(snapshot.orders.len(), 1);
        assert_eq!(snapshot.skipped.len(), 3);
    }
}
