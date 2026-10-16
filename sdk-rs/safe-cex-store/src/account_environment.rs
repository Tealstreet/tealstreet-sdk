//! Pure environment helpers from createExchangeFromAccount.ts.
//! Account option construction and exchange creation remain separate work.
use safe_cex_num::js_trim;
use safe_cex_types::ExchangeOptionsExtraKucoinBroker;

pub fn get_public_env(name: &str, lookup: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let value = lookup(name)?;
    let trimmed = js_trim(&value);
    (!trimmed.is_empty()).then(|| trimmed.to_owned())
}

pub fn parse_kucoin_broker_verify(value: Option<&str>) -> Option<bool> {
    let value = value.filter(|value| !value.is_empty())?;
    match js_trim(value).to_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

pub fn is_lighter_exchange(exchange_name: &str) -> bool {
    exchange_name == "lighter" || exchange_name == "lighter-robinhood"
}

pub fn get_kucoin_broker_env(
    prefix: &str,
    suffix: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<String> {
    get_public_env(&format!("{prefix}_{suffix}"), lookup)
        .or_else(|| get_public_env(&format!("NEXT_PUBLIC_KUCOIN_BROKER_{suffix}"), lookup))
}

pub fn resolve_public_kucoin_broker_config(
    exchange: &str,
    lookup: &dyn Fn(&str) -> Option<String>,
) -> Option<ExchangeOptionsExtraKucoinBroker> {
    let prefix = match exchange {
        "kucoin" => "NEXT_PUBLIC_KUCOIN_FUTURES_BROKER",
        "kucoinspot" => "NEXT_PUBLIC_KUCOIN_SPOT_BROKER",
        _ => return None,
    };
    let partner = get_kucoin_broker_env(prefix, "PARTNER", lookup);
    let broker_key = get_kucoin_broker_env(prefix, "KEY", lookup);
    let broker_name = get_kucoin_broker_env(prefix, "NAME", lookup);
    // The source reads all three fields before checking required presence.
    let (Some(partner), Some(broker_key), Some(broker_name)) = (partner, broker_key, broker_name)
    else {
        return None;
    };
    let verify = get_kucoin_broker_env(prefix, "VERIFY", lookup);
    Some(ExchangeOptionsExtraKucoinBroker {
        partner,
        broker_key,
        broker_name,
        verify: parse_kucoin_broker_verify(verify.as_deref()),
    })
}
