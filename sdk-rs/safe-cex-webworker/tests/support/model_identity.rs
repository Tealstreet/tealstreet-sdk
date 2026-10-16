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
    if let Some(price) = &v.limits.price {
        out["limits"]["price"] = fields!(price; "min"=>min);
    }
    if let Some(option) = &v.option {
        use safe_cex_types::{OptionRight, OptionTermsSettlement, OptionTermsStyle};
        out["option"] = fields!(option; "underlying"=>underlying, "expiry"=>expiry,
            "strike"=>strike, "premiumAsset"=>premium_asset, "settleAsset"=>settle_asset);
        out["option"]["right"] = json!(match option.right {
            OptionRight::Call => "call",
            OptionRight::Put => "put",
        });
        out["option"]["style"] = json!(match option.style {
            OptionTermsStyle::European => "european",
        });
        out["option"]["settlement"] = json!(match option.settlement {
            OptionTermsSettlement::Cash => "cash",
        });
    }
    out
}
