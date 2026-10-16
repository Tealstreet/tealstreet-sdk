use safe_cex_types::Market;
use serde_json::{Value, json};

macro_rules! fields {
    ($v:expr; $($wire:literal => $field:ident),* $(,)?) => {json!({$($wire: &$v.$field),*})};
}
pub fn market(v: &Market) -> Value {
    let mut out = fields!(v; "id"=>id,"symbol"=>symbol,"prettySymbol"=>pretty_symbol,"base"=>base,"quote"=>quote,"active"=>active,
        "contractSize"=>contract_size,"isExpiring"=>is_expiring,"isInverse"=>is_inverse,"isSpot"=>is_spot,"isPerpetual"=>is_perpetual,"isReadOnly"=>is_read_only,
        "makerFee"=>maker_fee,"takerFee"=>taker_fee,"info"=>info);
    out["precision"] = fields!(&v.precision; "amount"=>amount,"price"=>price);
    out["limits"] = json!({"amount":fields!(&v.limits.amount; "min"=>min,"max"=>max),"leverage":fields!(&v.limits.leverage; "min"=>min,"max"=>max)});
    out
}
