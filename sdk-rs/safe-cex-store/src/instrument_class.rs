use safe_cex_num::{ProtoMember, TableHit, js_table_lookup, js_trim};
use safe_cex_types::Market;
use safe_cex_types::retained_json::RetainedJsonInput as Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetClass {
    Crypto,
    Equity,
    Etf,
    Commodity,
    Fx,
    Index,
    // ts-bug: bybitInstrumentClass inherits Object.prototype keys.
    PrototypeProperty(ProtoMember),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstrumentClassResult {
    Classified {
        asset_class: AssetClass,
        venue_label: String,
        underlying_ticker: Option<String>,
        market_region: Option<String>,
    },
    Unknown {
        reason: String,
    },
}

pub type VenueInstrumentClassFn = fn(&Market) -> InstrumentClassResult;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentClassVenue {
    Bybit,
    Bybitdemo,
    Bybitinverse,
    Bybitspot,
}
impl InstrumentClassVenue {
    fn name(self) -> &'static str {
        match self {
            Self::Bybit => "bybit",
            Self::Bybitdemo => "bybitdemo",
            Self::Bybitinverse => "bybitinverse",
            Self::Bybitspot => "bybitspot",
        }
    }
}
pub type InstrumentClassRegistry = [(InstrumentClassVenue, VenueInstrumentClassFn); 4];
pub const VENUE_INSTRUMENT_CLASS: InstrumentClassRegistry = [
    (InstrumentClassVenue::Bybit, bybit_instrument_class),
    (InstrumentClassVenue::Bybitdemo, bybit_instrument_class),
    (InstrumentClassVenue::Bybitinverse, bybit_instrument_class),
    (InstrumentClassVenue::Bybitspot, bybit_instrument_class),
];

pub fn is_instrument_class_venue(exchange: &str) -> bool {
    VENUE_INSTRUMENT_CLASS
        .iter()
        .any(|(name, _)| name.name() == exchange)
}
pub fn resolve_venue_instrument_class(
    exchange: InstrumentClassVenue,
    market: &Market,
) -> InstrumentClassResult {
    classify_market(exchange.name(), market)
}
pub fn classify_market(exchange: &str, market: &Market) -> InstrumentClassResult {
    VENUE_INSTRUMENT_CLASS
        .iter()
        .find(|(name, _)| name.name() == exchange)
        .map(|(_, classify)| classify(market))
        .unwrap_or_else(|| InstrumentClassResult::Unknown {
            reason: format!("no instrument classifier implemented for {exchange}"),
        })
}
pub fn is_non_crypto_instrument(result: &InstrumentClassResult) -> Option<bool> {
    match result {
        InstrumentClassResult::Classified { asset_class, .. } => {
            Some(*asset_class != AssetClass::Crypto)
        }
        _ => None,
    }
}
pub fn venues_with_instrument_class_fn() -> Vec<InstrumentClassVenue> {
    VENUE_INSTRUMENT_CLASS
        .iter()
        .map(|(name, _)| *name)
        .collect()
}

fn text(value: Option<&Input>) -> Result<Option<String>, std::string::FromUtf16Error> {
    value
        .and_then(Input::as_text)
        .map(String::from_utf16)
        .transpose()
        .map(|value| {
            value
                .map(|value| js_trim(&value).to_owned())
                .filter(|value| !value.is_empty())
        })
}
pub fn bybit_instrument_class(market: &Market) -> InstrumentClassResult {
    let unknown = |reason: String| InstrumentClassResult::Unknown { reason };
    let Some(info) = market
        .info
        .as_ref()
        .filter(|v| v.is_object() || v.is_array())
    else {
        return unknown("bybit market carries no instruments-info payload".into());
    };
    let raw = info.get("symbolType");
    let Some(raw) = raw.filter(|v| !matches!(v, Input::Null | Input::Undefined)) else {
        return unknown("bybit instrument has no symbolType field".into());
    };
    let Some(label) = raw.as_text() else {
        let kind = match raw {
            Input::Bool(_) => "boolean",
            Input::Float(_) | Input::Lexeme(_) => "number",
            _ => "object",
        };
        return unknown(format!("bybit symbolType is {kind}, not a string"));
    };
    let Ok(label) = String::from_utf16(label) else {
        return unknown("bybit symbolType is outside the UTF-8 classification boundary".into());
    };
    let label = js_trim(&label);
    let table = [
        ("", AssetClass::Crypto),
        ("innovation", AssetClass::Crypto),
        ("stock", AssetClass::Equity),
        ("ETF", AssetClass::Etf),
        ("commodity", AssetClass::Commodity),
        ("forex", AssetClass::Fx),
    ];
    let asset_class = match js_table_lookup(&table, label) {
        TableHit::Mapped(value) => *value,
        // ts-bug: retain stock inherited identity, including the distinct prototype object.
        TableHit::Inherited(member) => AssetClass::PrototypeProperty(member),
        TableHit::Missing => return unknown(format!("unrecognised bybit symbolType \"{label}\"")),
    };
    let (Ok(underlying_ticker), Ok(market_region)) = (
        text(info.get("underlyingTicker")),
        text(info.get("marketRegion")),
    ) else {
        return unknown(
            "bybit instrument metadata is outside the UTF-8 classification boundary".into(),
        );
    };
    InstrumentClassResult::Classified {
        asset_class,
        venue_label: label.into(),
        underlying_ticker,
        market_region,
    }
}
pub fn asset_class_label(ac: AssetClass) -> &'static str {
    match ac {
        AssetClass::Crypto => "crypto",
        AssetClass::Equity => "tokenized equity",
        AssetClass::Etf => "tokenized ETF",
        AssetClass::Commodity => "tokenized commodity",
        AssetClass::Fx => "tokenized FX pair",
        AssetClass::Index => "tokenized index",
        AssetClass::PrototypeProperty(_) => "undefined",
    }
}
pub fn describe_instrument(symbol: &str, exchange: &str, result: &InstrumentClassResult) -> String {
    match result {
        InstrumentClassResult::Unknown { reason } => format!("{symbol} on {exchange}: {reason}"),
        InstrumentClassResult::Classified {
            asset_class,
            underlying_ticker,
            ..
        } => {
            let base = format!(
                "{symbol} on {exchange}: {}",
                asset_class_label(*asset_class)
            );
            if *asset_class == AssetClass::Crypto {
                format!("{base} perpetual")
            } else {
                let underlying = underlying_ticker
                    .as_ref()
                    .filter(|s| !s.is_empty())
                    .map(|s| format!(" ({s})"))
                    .unwrap_or_default();
                format!("{base}{underlying} — may require signing a trading agreement")
            }
        }
    }
}
pub fn tradfi_preflight_message(
    symbol: &str,
    exchange: &str,
    result: &InstrumentClassResult,
) -> Option<String> {
    let InstrumentClassResult::Classified {
        asset_class,
        underlying_ticker,
        ..
    } = result
    else {
        return None;
    };
    if *asset_class == AssetClass::Crypto {
        return None;
    }
    let underlying = underlying_ticker
        .as_ref()
        .filter(|s| !s.is_empty())
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    Some(format!(
        "{symbol} on {exchange} is a {}{underlying}. Opening a position may require an agreement you have not signed with the venue — place the order through the venue's own UI first to accept it.",
        asset_class_label(*asset_class)
    ))
}
