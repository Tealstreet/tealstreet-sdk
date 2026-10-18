// Test-only observations of returned native fields, before JSON erases numbers/identity.
use serde_json::{Value, json};

pub fn bits(number: f64) -> String {
    if number.is_nan() {
        "NaN".into()
    } else {
        format!("{:016x}", number.to_bits())
    }
}
pub fn project(value: Value) -> Value {
    match value {
        Value::Number(number) => {
            json!({"bits":bits(number.as_f64().unwrap_or_else(|| number.to_string().parse::<f64>().unwrap()))})
        }
        Value::Array(values) => Value::Array(values.into_iter().map(project).collect()),
        Value::Object(values) => Value::Object(
            values
                .into_iter()
                .map(|(key, value)| (key, project(value)))
                .collect(),
        ),
        other => other,
    }
}
pub fn balances(rows: &[crate::account_helpers::AssetBalanceUpdate]) -> Value {
    json!(rows.iter().map(|row|json!({"asset":row.asset,"bits":[bits(row.total),bits(row.free),bits(row.used)]})).collect::<Vec<_>>())
}
pub fn execution(
    result: Result<
        Option<(String, crate::account_helpers::ExecutionReportUpdate)>,
        safe_cex_num::JsCoercionError,
    >,
) -> Value {
    match result {
        Err(error) => json!({"error":error.name}),
        Ok(None) => Value::Null,
        Ok(Some((id, row))) => {
            let status = match row.status {
                safe_cex_num::TableHit::Mapped(status) => json!({"kind":"mapped","value":status}),
                safe_cex_num::TableHit::Inherited(member) => {
                    json!({"kind":"inherited","text":member.js_string(),"arity":member.length()})
                }
                safe_cex_num::TableHit::Missing => {
                    panic!("execution status always has source fallback")
                }
            };
            json!({"id":id,"status":status,"bits":[bits(row.filled),bits(row.remaining)]})
        }
    }
}
pub fn cumulative(
    result: Result<
        Option<crate::account_helpers::CumulativeOrderFill>,
        safe_cex_num::JsCoercionError,
    >,
) -> Value {
    match result {
        Err(error) => json!({"error":error.name}),
        Ok(None) => Value::Null,
        Ok(Some(row)) => {
            json!({"id":row.order_id,"status":row.status,"bits":[bits(row.filled),bits(row.remaining),bits(row.last_update)]})
        }
    }
}
pub fn effects<'a>(
    input: &'a Value,
    result: Result<
        Vec<crate::account_helpers::AccountUpdateEffect<'a>>,
        safe_cex_num::JsCoercionError,
    >,
) -> Value {
    match result {
        Err(error) => json!({"error":error.name}),
        Ok(rows) => json!(
            rows.into_iter()
                .map(|row| match row {
                    crate::account_helpers::AccountUpdateEffect::ApplyBalances(rows) =>
                        json!({"kind":"balances","rows":balances(&rows)}),
                    crate::account_helpers::AccountUpdateEffect::EmitAccount(data) =>
                        json!({"kind":"emit","same":std::ptr::eq(input,data)}),
                })
                .collect::<Vec<_>>()
        ),
    }
}
pub fn ready<F: std::future::Future>(future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    match future
        .as_mut()
        .poll(&mut std::task::Context::from_waker(std::task::Waker::noop()))
    {
        std::task::Poll::Ready(output) => output,
        std::task::Poll::Pending => panic!("local native signer must settle without host I/O"),
    }
}

pub fn position(row: &Value) -> safe_cex_types::Position {
    use safe_cex_types::ModelField;
    safe_cex_types::Position {
        id: "fixture-position".into(),
        symbol: row["symbol"].as_str().unwrap().into(),
        side: ModelField::Value(serde_json::from_value(row["side"].clone()).unwrap()),
        entry_price: 0.0,
        notional: 0.0,
        leverage: 1.0,
        margin_type: ModelField::Absent,
        position_mode: ModelField::Absent,
        unrealized_pnl: row["previous"]
            .as_f64()
            .unwrap_or_else(|| row["previous"].as_str().unwrap().parse::<f64>().unwrap()),
        realized_pnl: ModelField::Absent,
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_size: ModelField::Absent,
        info: ModelField::Absent,
        extra: ModelField::Absent,
        last_update: ModelField::Absent,
        option: ModelField::Absent,
    }
}
