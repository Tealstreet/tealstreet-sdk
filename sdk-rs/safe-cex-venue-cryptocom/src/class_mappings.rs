//! Native switch bindings preserve imported enum property reads and string identity.
use crate::native_bindings::BoundHost;
fn imported<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    owner: &str,
    key: &str,
) -> Result<V, E> {
    h.get(&(h.ops.value.global)(owner)?, key)
}
pub fn member<V: Clone + 'static, E: 'static>(
    h: &BoundHost<V, E>,
    name: &str,
    value: V,
    private: bool,
) -> Result<V, E> {
    let input_enum = match name {
        "mapOrderType" if !private => Some("OrderType"),
        "mapTimeInForce" if !private => Some("OrderTimeInForce"),
        "mapStopTriggerType" => Some("OrderStopTriggerType"),
        _ => None,
    };
    let (cases, default, output): (&[(&str, &str)], &str, Option<&str>) = match name {
        "mapOrderType" if !private => (
            &[
                ("Market", "MARKET"),
                ("Limit", "LIMIT"),
                ("StopLoss", "STOP_LOSS"),
                ("TakeProfit", "TAKE_PROFIT"),
            ],
            "LIMIT",
            None,
        ),
        "mapTimeInForce" if !private => (
            &[
                ("ImmediateOrCancel", "IMMEDIATE_OR_CANCEL"),
                ("FillOrKill", "FILL_OR_KILL"),
                ("PostOnly", "POST_ONLY"),
            ],
            "GOOD_TILL_CANCEL",
            None,
        ),
        "mapStopTriggerType" => (
            &[("IndexPrice", "INDEX_PRICE"), ("LastPrice", "LAST_PRICE")],
            "MARK_PRICE",
            None,
        ),
        "mapOrderType" | "mapCryptocomOrderType" => (
            &[
                ("MARKET", "Market"),
                ("LIMIT", "Limit"),
                ("STOP_LOSS", "StopLoss"),
                ("STOP_LIMIT", "StopLoss"),
                ("TAKE_PROFIT", "TakeProfit"),
                ("TAKE_PROFIT_LIMIT", "TakeProfit"),
            ],
            "Limit",
            Some("OrderType"),
        ),
        "mapOrderStatus" => (
            &[
                ("ACTIVE", "Open"),
                ("PENDING", "Open"),
                ("FILLED", "Closed"),
                ("CANCELED", "Canceled"),
                ("EXPIRED", "Canceled"),
                ("REJECTED", "Canceled"),
            ],
            "Open",
            Some("OrderStatus"),
        ),
        "mapTimeInForce" | "mapCryptocomTimeInForce" => (
            &[
                ("IMMEDIATE_OR_CANCEL", "ImmediateOrCancel"),
                ("FILL_OR_KILL", "FillOrKill"),
                ("POST_ONLY", "PostOnly"),
            ],
            "GoodTillCancel",
            Some("OrderTimeInForce"),
        ),
        _ => unreachable!("every native mapping binding has a source switch"),
    };
    let mut result = default;
    for (case, target) in cases {
        let test = if let Some(owner) = input_enum {
            imported(h, owner, case)?
        } else {
            (h.ops.value.literal)(case)
        };
        if (h.ops.value.equal)(&value, &test) {
            result = target;
            break;
        }
    }
    if let Some(owner) = output {
        imported(h, owner, result)
    } else {
        Ok((h.ops.value.literal)(result))
    }
}
