//! Venue-specific string unions, distinct from shared normalized order enums.

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum ApexMarket {
    #[serde(rename = "BTC-USDT")]
    BtcUsdt,
    #[serde(rename = "BTC-USDC")]
    BtcUsdc,
    #[serde(rename = "BTC-USD")]
    BtcUsd,
    #[serde(rename = "ETH-USD")]
    EthUsd,
    #[serde(rename = "ETH-USDC")]
    EthUsdc,
    #[serde(rename = "LINK-USD")]
    LinkUsd,
    #[serde(rename = "AAVE-USD")]
    AaveUsd,
    #[serde(rename = "UNI-USD")]
    UniUsd,
    #[serde(rename = "SUSHI-USD")]
    SushiUsd,
    #[serde(rename = "SOL-USD")]
    SolUsd,
    #[serde(rename = "YFI-USD")]
    YfiUsd,
    #[serde(rename = "1INCH-USD")]
    OneinchUsd,
    #[serde(rename = "AVAX-USD")]
    AvaxUsd,
    #[serde(rename = "SNX-USD")]
    SnxUsd,
    #[serde(rename = "CRV-USD")]
    CrvUsd,
    #[serde(rename = "UMA-USD")]
    UmaUsd,
    #[serde(rename = "DOT-USD")]
    DotUsd,
    #[serde(rename = "DOGE-USD")]
    DogeUsd,
    #[serde(rename = "MATIC-USD")]
    MaticUsd,
    #[serde(rename = "MKR-USD")]
    MkrUsd,
    #[serde(rename = "FIL-USD")]
    FilUsd,
    #[serde(rename = "ADA-USD")]
    AdaUsd,
    #[serde(rename = "ATOM-USD")]
    AtomUsd,
    #[serde(rename = "COMP-USD")]
    CompUsd,
    #[serde(rename = "BCH-USD")]
    BchUsd,
    #[serde(rename = "LTC-USD")]
    LtcUsd,
    #[serde(rename = "EOS-USD")]
    EosUsd,
    #[serde(rename = "ALGO-USD")]
    AlgoUsd,
    #[serde(rename = "ZRX-USD")]
    ZrxUsd,
    #[serde(rename = "XMR-USD")]
    XmrUsd,
    #[serde(rename = "ZEC-USD")]
    ZecUsd,
    #[serde(rename = "ENJ-USD")]
    EnjUsd,
    #[serde(rename = "ETC-USD")]
    EtcUsd,
    #[serde(rename = "XLM-USD")]
    XlmUsd,
    #[serde(rename = "TRX-USD")]
    TrxUsd,
    #[serde(rename = "XTZ-USD")]
    XtzUsd,
    #[serde(rename = "HNT-USD")]
    HntUsd,
}
impl<'de> serde::Deserialize<'de> for ApexMarket {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "BTC-USDT" => Ok(Self::BtcUsdt),
            "BTC-USDC" => Ok(Self::BtcUsdc),
            "BTC-USD" => Ok(Self::BtcUsd),
            "ETH-USD" => Ok(Self::EthUsd),
            "ETH-USDC" => Ok(Self::EthUsdc),
            "LINK-USD" => Ok(Self::LinkUsd),
            "AAVE-USD" => Ok(Self::AaveUsd),
            "UNI-USD" => Ok(Self::UniUsd),
            "SUSHI-USD" => Ok(Self::SushiUsd),
            "SOL-USD" => Ok(Self::SolUsd),
            "YFI-USD" => Ok(Self::YfiUsd),
            "1INCH-USD" => Ok(Self::OneinchUsd),
            "AVAX-USD" => Ok(Self::AvaxUsd),
            "SNX-USD" => Ok(Self::SnxUsd),
            "CRV-USD" => Ok(Self::CrvUsd),
            "UMA-USD" => Ok(Self::UmaUsd),
            "DOT-USD" => Ok(Self::DotUsd),
            "DOGE-USD" => Ok(Self::DogeUsd),
            "MATIC-USD" => Ok(Self::MaticUsd),
            "MKR-USD" => Ok(Self::MkrUsd),
            "FIL-USD" => Ok(Self::FilUsd),
            "ADA-USD" => Ok(Self::AdaUsd),
            "ATOM-USD" => Ok(Self::AtomUsd),
            "COMP-USD" => Ok(Self::CompUsd),
            "BCH-USD" => Ok(Self::BchUsd),
            "LTC-USD" => Ok(Self::LtcUsd),
            "EOS-USD" => Ok(Self::EosUsd),
            "ALGO-USD" => Ok(Self::AlgoUsd),
            "ZRX-USD" => Ok(Self::ZrxUsd),
            "XMR-USD" => Ok(Self::XmrUsd),
            "ZEC-USD" => Ok(Self::ZecUsd),
            "ENJ-USD" => Ok(Self::EnjUsd),
            "ETC-USD" => Ok(Self::EtcUsd),
            "XLM-USD" => Ok(Self::XlmUsd),
            "TRX-USD" => Ok(Self::TrxUsd),
            "XTZ-USD" => Ok(Self::XtzUsd),
            "HNT-USD" => Ok(Self::HntUsd),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "BTC-USDT",
                    "BTC-USDC",
                    "BTC-USD",
                    "ETH-USD",
                    "ETH-USDC",
                    "LINK-USD",
                    "AAVE-USD",
                    "UNI-USD",
                    "SUSHI-USD",
                    "SOL-USD",
                    "YFI-USD",
                    "1INCH-USD",
                    "AVAX-USD",
                    "SNX-USD",
                    "CRV-USD",
                    "UMA-USD",
                    "DOT-USD",
                    "DOGE-USD",
                    "MATIC-USD",
                    "MKR-USD",
                    "FIL-USD",
                    "ADA-USD",
                    "ATOM-USD",
                    "COMP-USD",
                    "BCH-USD",
                    "LTC-USD",
                    "EOS-USD",
                    "ALGO-USD",
                    "ZRX-USD",
                    "XMR-USD",
                    "ZEC-USD",
                    "ENJ-USD",
                    "ETC-USD",
                    "XLM-USD",
                    "TRX-USD",
                    "XTZ-USD",
                    "HNT-USD",
                ],
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}
impl<'de> serde::Deserialize<'de> for OrderSide {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "BUY" => Ok(Self::Buy),
            "SELL" => Ok(Self::Sell),
            _ => Err(serde::de::Error::unknown_variant(&value, &["BUY", "SELL"])),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OrderType {
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "CONDITION")]
    Condition,
}
impl<'de> serde::Deserialize<'de> for OrderType {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "LIMIT" => Ok(Self::Limit),
            "MARKET" => Ok(Self::Market),
            "CONDITION" => Ok(Self::Condition),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["LIMIT", "MARKET", "CONDITION"],
            )),
        }
    }
}

// TS exports Market as both the ApexMarket type alias and enum namespace alias.
pub type Market = ApexMarket;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum TimeInForce {
    #[serde(rename = "GTT")]
    Gtt,
    #[serde(rename = "FOK")]
    Fok,
    #[serde(rename = "IOC")]
    Ioc,
}
impl<'de> serde::Deserialize<'de> for TimeInForce {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "GTT" => Ok(Self::Gtt),
            "FOK" => Ok(Self::Fok),
            "IOC" => Ok(Self::Ioc),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["GTT", "FOK", "IOC"],
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum OrderStatus {
    #[serde(rename = "PENDING")]
    Pending,
    #[serde(rename = "OPEN")]
    Open,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "UNTRIGGERED")]
    Untriggered,
}
impl<'de> serde::Deserialize<'de> for OrderStatus {
    fn deserialize<D: serde::Deserializer<'de>>(decoder: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(decoder)?;
        match value.as_str() {
            "PENDING" => Ok(Self::Pending),
            "OPEN" => Ok(Self::Open),
            "FILLED" => Ok(Self::Filled),
            "CANCELED" => Ok(Self::Canceled),
            "UNTRIGGERED" => Ok(Self::Untriggered),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &["PENDING", "OPEN", "FILLED", "CANCELED", "UNTRIGGERED"],
            )),
        }
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for ApexMarket {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { BtcUsdt, BtcUsdc, BtcUsd, EthUsd, EthUsdc, LinkUsd, AaveUsd, UniUsd, SushiUsd, SolUsd, YfiUsd, OneinchUsd, AvaxUsd, SnxUsd, CrvUsd, UmaUsd, DotUsd, DogeUsd, MaticUsd, MkrUsd, FilUsd, AdaUsd, AtomUsd, CompUsd, BchUsd, LtcUsd, EosUsd, AlgoUsd, ZrxUsd, XmrUsd, ZecUsd, EnjUsd, EtcUsd, XlmUsd, TrxUsd, XtzUsd, HntUsd });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderSide {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Buy, Sell });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderType {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Limit, Market, Condition });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for TimeInForce {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Gtt, Fok, Ioc });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for OrderStatus {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Pending, Open, Filled, Canceled, Untriggered });
    }
}
