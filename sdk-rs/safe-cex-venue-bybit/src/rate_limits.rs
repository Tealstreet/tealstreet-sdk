use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitVipLevel {
    NoVIP,
    VIP1,
    VIP2,
    VIP3,
    VIP4,
    VIP5,
    VIPSupreme,
    PRO1,
    PRO2,
    PRO3,
    PRO4,
    PRO5,
    PRO6,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitProductType {
    Futures,
    Option,
    Spot,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BybitAccountType {
    ClassicAndUTA,
    UTAPro,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitRateLimitAccountRates {
    pub classic_and_uta: f64,
    pub uta_pro: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct BybitRateLimitRow {
    pub level: BybitVipLevel,
    pub futures: BybitRateLimitAccountRates,
    pub option: BybitRateLimitAccountRates,
    pub spot: BybitRateLimitAccountRates,
}

pub static BYBIT_RATE_LIMIT_TABLE: LazyLock<Vec<BybitRateLimitRow>> = LazyLock::new(|| {
    serde_json::from_str(r#"[{"level": "No VIP", "futures": {"classicAndUTA": 10, "utaPro": 10}, "option": {"classicAndUTA": 10, "utaPro": 10}, "spot": {"classicAndUTA": 20, "utaPro": 20}}, {"level": "VIP-1", "futures": {"classicAndUTA": 20, "utaPro": 20}, "option": {"classicAndUTA": 20, "utaPro": 20}, "spot": {"classicAndUTA": 25, "utaPro": 25}}, {"level": "VIP-2", "futures": {"classicAndUTA": 40, "utaPro": 40}, "option": {"classicAndUTA": 40, "utaPro": 40}, "spot": {"classicAndUTA": 30, "utaPro": 30}}, {"level": "VIP-3", "futures": {"classicAndUTA": 60, "utaPro": 60}, "option": {"classicAndUTA": 60, "utaPro": 60}, "spot": {"classicAndUTA": 40, "utaPro": 40}}, {"level": "VIP-4", "futures": {"classicAndUTA": 60, "utaPro": 60}, "option": {"classicAndUTA": 60, "utaPro": 60}, "spot": {"classicAndUTA": 40, "utaPro": 40}}, {"level": "VIP-5", "futures": {"classicAndUTA": 60, "utaPro": 60}, "option": {"classicAndUTA": 60, "utaPro": 60}, "spot": {"classicAndUTA": 40, "utaPro": 40}}, {"level": "VIP-Supreme", "futures": {"classicAndUTA": 60, "utaPro": 60}, "option": {"classicAndUTA": 60, "utaPro": 60}, "spot": {"classicAndUTA": 40, "utaPro": 40}}, {"level": "PRO-1", "futures": {"classicAndUTA": 100, "utaPro": 150}, "option": {"classicAndUTA": 100, "utaPro": 150}, "spot": {"classicAndUTA": 50, "utaPro": 150}}, {"level": "PRO-2", "futures": {"classicAndUTA": 150, "utaPro": 200}, "option": {"classicAndUTA": 150, "utaPro": 200}, "spot": {"classicAndUTA": 75, "utaPro": 200}}, {"level": "PRO-3", "futures": {"classicAndUTA": 200, "utaPro": 250}, "option": {"classicAndUTA": 200, "utaPro": 250}, "spot": {"classicAndUTA": 100, "utaPro": 250}}, {"level": "PRO-4", "futures": {"classicAndUTA": 200, "utaPro": 300}, "option": {"classicAndUTA": 200, "utaPro": 300}, "spot": {"classicAndUTA": 100, "utaPro": 300}}, {"level": "PRO-5", "futures": {"classicAndUTA": 200, "utaPro": 300}, "option": {"classicAndUTA": 200, "utaPro": 300}, "spot": {"classicAndUTA": 100, "utaPro": 300}}, {"level": "PRO-6", "futures": {"classicAndUTA": 200, "utaPro": 300}, "option": {"classicAndUTA": 200, "utaPro": 300}, "spot": {"classicAndUTA": 100, "utaPro": 300}}]"#).expect("static Bybit rate table")
});

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitVipLevelWire {
    #[serde(rename = "No VIP")]
    NoVIP,
    #[serde(rename = "VIP-1")]
    VIP1,
    #[serde(rename = "VIP-2")]
    VIP2,
    #[serde(rename = "VIP-3")]
    VIP3,
    #[serde(rename = "VIP-4")]
    VIP4,
    #[serde(rename = "VIP-5")]
    VIP5,
    #[serde(rename = "VIP-Supreme")]
    VIPSupreme,
    #[serde(rename = "PRO-1")]
    PRO1,
    #[serde(rename = "PRO-2")]
    PRO2,
    #[serde(rename = "PRO-3")]
    PRO3,
    #[serde(rename = "PRO-4")]
    PRO4,
    #[serde(rename = "PRO-5")]
    PRO5,
    #[serde(rename = "PRO-6")]
    PRO6,
}
impl<'de> Deserialize<'de> for BybitVipLevel {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire = BybitVipLevelWire::deserialize(
            serde::de::value::StringDeserializer::<D::Error>::new(value),
        )?;
        Ok(match wire {
            BybitVipLevelWire::NoVIP => Self::NoVIP,
            BybitVipLevelWire::VIP1 => Self::VIP1,
            BybitVipLevelWire::VIP2 => Self::VIP2,
            BybitVipLevelWire::VIP3 => Self::VIP3,
            BybitVipLevelWire::VIP4 => Self::VIP4,
            BybitVipLevelWire::VIP5 => Self::VIP5,
            BybitVipLevelWire::VIPSupreme => Self::VIPSupreme,
            BybitVipLevelWire::PRO1 => Self::PRO1,
            BybitVipLevelWire::PRO2 => Self::PRO2,
            BybitVipLevelWire::PRO3 => Self::PRO3,
            BybitVipLevelWire::PRO4 => Self::PRO4,
            BybitVipLevelWire::PRO5 => Self::PRO5,
            BybitVipLevelWire::PRO6 => Self::PRO6,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_vip_level_accepted_set() {
    let _exhaustive = |value: BybitVipLevel| match value {
        BybitVipLevel::NoVIP
        | BybitVipLevel::VIP1
        | BybitVipLevel::VIP2
        | BybitVipLevel::VIP3
        | BybitVipLevel::VIP4
        | BybitVipLevel::VIP5
        | BybitVipLevel::VIPSupreme
        | BybitVipLevel::PRO1
        | BybitVipLevel::PRO2
        | BybitVipLevel::PRO3
        | BybitVipLevel::PRO4
        | BybitVipLevel::PRO5
        | BybitVipLevel::PRO6 => (),
    };
    crate::union_audit::check::<BybitVipLevel, BybitVipLevelWire>("BybitVipLevel");
}

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitProductTypeWire {
    #[serde(rename = "futures")]
    Futures,
    #[serde(rename = "option")]
    Option,
    #[serde(rename = "spot")]
    Spot,
}
impl<'de> Deserialize<'de> for BybitProductType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire = BybitProductTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))?;
        Ok(match wire {
            BybitProductTypeWire::Futures => Self::Futures,
            BybitProductTypeWire::Option => Self::Option,
            BybitProductTypeWire::Spot => Self::Spot,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_product_type_accepted_set() {
    let _exhaustive = |value: BybitProductType| match value {
        BybitProductType::Futures | BybitProductType::Option | BybitProductType::Spot => (),
    };
    crate::union_audit::check::<BybitProductType, BybitProductTypeWire>("BybitProductType");
}

// Private derived decoder supplies canonical serde spelling metadata. Only the
// string-required public Deserialize implementation can construct the public type.
#[derive(Serialize, Deserialize)]
enum BybitAccountTypeWire {
    #[serde(rename = "classicAndUTA")]
    ClassicAndUTA,
    #[serde(rename = "utaPro")]
    UTAPro,
}
impl<'de> Deserialize<'de> for BybitAccountType {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = <String as Deserialize>::deserialize(d)?;
        let wire = BybitAccountTypeWire::deserialize(serde::de::value::StringDeserializer::<
            D::Error,
        >::new(value))?;
        Ok(match wire {
            BybitAccountTypeWire::ClassicAndUTA => Self::ClassicAndUTA,
            BybitAccountTypeWire::UTAPro => Self::UTAPro,
        })
    }
}
#[cfg(test)]
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn bybit_account_type_accepted_set() {
    let _exhaustive = |value: BybitAccountType| match value {
        BybitAccountType::ClassicAndUTA | BybitAccountType::UTAPro => (),
    };
    crate::union_audit::check::<BybitAccountType, BybitAccountTypeWire>("BybitAccountType");
}

impl Serialize for BybitVipLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::NoVIP => BybitVipLevelWire::NoVIP,
            Self::VIP1 => BybitVipLevelWire::VIP1,
            Self::VIP2 => BybitVipLevelWire::VIP2,
            Self::VIP3 => BybitVipLevelWire::VIP3,
            Self::VIP4 => BybitVipLevelWire::VIP4,
            Self::VIP5 => BybitVipLevelWire::VIP5,
            Self::VIPSupreme => BybitVipLevelWire::VIPSupreme,
            Self::PRO1 => BybitVipLevelWire::PRO1,
            Self::PRO2 => BybitVipLevelWire::PRO2,
            Self::PRO3 => BybitVipLevelWire::PRO3,
            Self::PRO4 => BybitVipLevelWire::PRO4,
            Self::PRO5 => BybitVipLevelWire::PRO5,
            Self::PRO6 => BybitVipLevelWire::PRO6,
        };
        wire.serialize(serializer)
    }
}

impl Serialize for BybitProductType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::Futures => BybitProductTypeWire::Futures,
            Self::Option => BybitProductTypeWire::Option,
            Self::Spot => BybitProductTypeWire::Spot,
        };
        wire.serialize(serializer)
    }
}

impl Serialize for BybitAccountType {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let wire = match self {
            Self::ClassicAndUTA => BybitAccountTypeWire::ClassicAndUTA,
            Self::UTAPro => BybitAccountTypeWire::UTAPro,
        };
        wire.serialize(serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitRateLimitAccountRates")]
struct BybitRateLimitAccountRatesWire {
    #[serde(rename = "classicAndUTA")]
    classic_and_uta: f64,
    #[serde(rename = "utaPro")]
    uta_pro: f64,
}
impl<'de> Deserialize<'de> for BybitRateLimitAccountRates {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitRateLimitAccountRatesWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitRateLimitAccountRates {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitRateLimitAccountRatesWire::serialize(self, serializer)
    }
}

// Private remote companion: the public path requires a map before serde can
// select its otherwise-permissive positional record visitor.
#[derive(Serialize, Deserialize)]
#[serde(remote = "BybitRateLimitRow")]
struct BybitRateLimitRowWire {
    level: BybitVipLevel,
    futures: BybitRateLimitAccountRates,
    option: BybitRateLimitAccountRates,
    spot: BybitRateLimitAccountRates,
}
impl<'de> Deserialize<'de> for BybitRateLimitRow {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let input = safe_cex_num::numeric_ingress::JsNumberInput::deserialize(d)?;
        input
            .decode_with(|decoder| {
                BybitRateLimitRowWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl Serialize for BybitRateLimitRow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        BybitRateLimitRowWire::serialize(self, serializer)
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitRateLimitAccountRates {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { classic_and_uta, uta_pro });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitRateLimitRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { level, futures, option, spot });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for BybitVipLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { NoVIP, VIP1, VIP2, VIP3, VIP4, VIP5, VIPSupreme, PRO1, PRO2, PRO3, PRO4, PRO5, PRO6 });
    }
}
