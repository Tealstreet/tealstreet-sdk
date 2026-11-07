use crate::retained_json::RetainedJsonInput as Input;
use serde::ser::SerializeStruct;

impl serde::Serialize for crate::Market {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::INFO_RECORD, 18)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("symbol", &self.symbol)?;
        if let Some(value) = &self.pretty_symbol {
            state.serialize_field("prettySymbol", value)?;
        }
        state.serialize_field("base", &self.base)?;
        state.serialize_field("quote", &self.quote)?;
        state.serialize_field("active", &self.active)?;
        state.serialize_field("precision", &self.precision)?;
        state.serialize_field("limits", &self.limits)?;
        if let Some(value) = &self.option {
            state.serialize_field("option", value)?;
        }
        if let Some(value) = &self.contract_size {
            state.serialize_field("contractSize", value)?;
        }
        if let Some(value) = &self.is_expiring {
            state.serialize_field("isExpiring", value)?;
        }
        if let Some(value) = &self.is_inverse {
            state.serialize_field("isInverse", value)?;
        }
        if let Some(value) = &self.is_spot {
            state.serialize_field("isSpot", value)?;
        }
        if let Some(value) = &self.is_perpetual {
            state.serialize_field("isPerpetual", value)?;
        }
        if let Some(value) = &self.is_read_only {
            state.serialize_field("isReadOnly", value)?;
        }
        match &self.info {
            None => {}
            Some(Input::Undefined) => state.skip_field("info")?,
            Some(value) => state.serialize_field("info", value)?,
        }
        if let Some(value) = &self.taker_fee {
            state.serialize_field("takerFee", value)?;
        }
        if let Some(value) = &self.maker_fee {
            state.serialize_field("makerFee", value)?;
        }
        if let Some(value) = &self.runtime.kind {
            state.serialize_field("type", value)?;
        }
        if let Some(value) = &self.runtime.spot {
            state.serialize_field("spot", value)?;
        }
        if let Some(value) = &self.runtime.swap {
            state.serialize_field("swap", value)?;
        }
        if let Some(value) = &self.runtime.future {
            state.serialize_field("future", value)?;
        }
        if let Some(value) = &self.runtime.contract {
            state.serialize_field("contract", value)?;
        }
        if let Some(value) = &self.runtime.linear {
            state.serialize_field("linear", value)?;
        }
        if let Some(value) = &self.runtime.inverse {
            state.serialize_field("inverse", value)?;
        }
        if let Some(value) = &self.runtime.fees {
            state.serialize_field("fees", value)?;
        }
        state.end()
    }
}

impl serde::Serialize for crate::Balance {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::INFO_RECORD, 7)?;
        state.serialize_field("used", &self.used)?;
        state.serialize_field("free", &self.free)?;
        state.serialize_field("total", &self.total)?;
        state.serialize_field("upnl", &self.upnl)?;
        match &self.info {
            None => {}
            Some(Input::Undefined) => state.skip_field("info")?,
            Some(value) => state.serialize_field("info", value)?,
        }
        if let Some(value) = &self.currencies {
            state.serialize_field("currencies", value)?;
        }
        if let Some(value) = &self.coins {
            state.serialize_field("coins", value)?;
        }
        state.end()
    }
}

impl serde::Serialize for crate::Ticker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::INFO_RECORD, 16)?;
        state.serialize_field("id", &self.id)?;
        state.serialize_field("symbol", &self.symbol)?;
        state.serialize_field("bid", &self.bid)?;
        state.serialize_field("ask", &self.ask)?;
        state.serialize_field("last", &self.last)?;
        state.serialize_field("mark", &self.mark)?;
        state.serialize_field("index", &self.index)?;
        if let Some(value) = &self.mid_price {
            state.serialize_field("midPrice", value)?;
        }
        state.serialize_field("percentage", &self.percentage)?;
        state.serialize_field("openInterest", &self.open_interest)?;
        state.serialize_field("fundingRate", &self.funding_rate)?;
        state.serialize_field("fundingTime", &self.funding_time)?;
        state.serialize_field("volume", &self.volume)?;
        state.serialize_field("quoteVolume", &self.quote_volume)?;
        if let Some(value) = &self.timestamp {
            state.serialize_field("timestamp", value)?;
        }
        match &self.info {
            None => {}
            Some(Input::Undefined) => state.skip_field("info")?,
            Some(value) => state.serialize_field("info", value)?,
        }
        state.end()
    }
}

pub const STORE_RECORD: &str = "$safe_cex::StoreRecord";

impl serde::Serialize for crate::DisplayedPosition {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 33)?;
        if let Some(value) = &self.key {
            state.serialize_field("key", value)?;
        }
        crate::serialize_position_fields(&self.position, &mut state)?;
        state.serialize_field("account", &self.account)?;
        if let Some(value) = &self.ticker {
            state.serialize_field("ticker", value)?;
        }
        if let Some(value) = &self.market {
            state.serialize_field("market", value)?;
        }
        if let Some(value) = &self.balance {
            state.serialize_field("balance", value)?;
        }
        if let Some(value) = &self.exchange_name {
            state.serialize_field("exchangeName", value)?;
        }
        if let Some(value) = &self.one_min_upnl {
            state.serialize_field("oneMinUpnl", value)?;
        }
        if let Some(value) = &self.five_min_upnl {
            state.serialize_field("fiveMinUpnl", value)?;
        }
        if let Some(value) = &self.has_active_chaser {
            state.serialize_field("hasActiveChaser", value)?;
        }
        if let Some(value) = &self.has_active_twap {
            state.serialize_field("hasActiveTWAP", value)?;
        }
        if let Some(value) = &self.is_fake_position {
            state.serialize_field("isFakePosition", value)?;
        }
        if let Some(value) = &self.chaser_size {
            state.serialize_field("chaserSize", value)?;
        }
        if let Some(value) = &self.has_full_stop {
            state.serialize_field("hasFullStop", value)?;
        }
        if let Some(value) = &self.has_full_tp {
            state.serialize_field("hasFullTP", value)?;
        }
        state.end()
    }
}
