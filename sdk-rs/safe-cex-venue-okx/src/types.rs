//! OKX protocol tables from okx.types.ts; demo and EEA hosts remain distinct.
use safe_cex_types::{OrderSide, OrderStatus, OrderType, PositionSide};
use serde::{Deserialize, Serialize};
pub const RECV_WINDOW: u64 = 20_000;
pub const BROKER_ID: &str = "e847386590ce4dBC";
// Checked-in declarations keep the source inventory visible to parity gates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "RegionWire")]
pub enum OkxRegion {
    Okx,
    Okxeu,
}
#[derive(Serialize)]
enum RegionWire {
    #[serde(rename = "okx")]
    Okx,
    #[serde(rename = "okxeu")]
    Okxeu,
}
#[derive(Deserialize)]
#[serde(remote = "RegionWire")]
enum RegionWireSchema {
    #[serde(rename = "okx")]
    Okx,
    #[serde(rename = "okxeu")]
    Okxeu,
}
impl<'de> Deserialize<'de> for RegionWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        RegionWireSchema::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}
impl From<OkxRegion> for RegionWire {
    fn from(value: OkxRegion) -> Self {
        match value {
            OkxRegion::Okx => Self::Okx,
            OkxRegion::Okxeu => Self::Okxeu,
        }
    }
}
impl<'de> Deserialize<'de> for OkxRegion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire =
            RegionWire::deserialize(serde::de::value::StrDeserializer::<D::Error>::new(&value))?;
        Ok(match wire {
            RegionWire::Okx => Self::Okx,
            RegionWire::Okxeu => Self::Okxeu,
        })
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(into = "InstrumentWire")]
pub enum OkxInstrumentType {
    Swap,
    Futures,
}
#[derive(Serialize)]
enum InstrumentWire {
    #[serde(rename = "SWAP")]
    Swap,
    #[serde(rename = "FUTURES")]
    Futures,
}
#[derive(Deserialize)]
#[serde(remote = "InstrumentWire")]
enum InstrumentWireSchema {
    #[serde(rename = "SWAP")]
    Swap,
    #[serde(rename = "FUTURES")]
    Futures,
}
impl<'de> Deserialize<'de> for InstrumentWire {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        InstrumentWireSchema::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}
impl From<OkxInstrumentType> for InstrumentWire {
    fn from(value: OkxInstrumentType) -> Self {
        match value {
            OkxInstrumentType::Swap => Self::Swap,
            OkxInstrumentType::Futures => Self::Futures,
        }
    }
}
impl<'de> Deserialize<'de> for OkxInstrumentType {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        let wire = InstrumentWire::deserialize(
            serde::de::value::StrDeserializer::<D::Error>::new(&value),
        )?;
        Ok(match wire {
            InstrumentWire::Swap => Self::Swap,
            InstrumentWire::Futures => Self::Futures,
        })
    }
}
pub const BASE_URLS: [(OkxRegion, [&str; 2]); 2] = [
    (
        OkxRegion::Okx,
        ["https://www.okx.com", "https://openapi.okx.com"],
    ),
    (
        OkxRegion::Okxeu,
        ["https://eea.okx.com", "https://eea.okx.com"],
    ),
];
pub const BASE_WS_URLS: [(OkxRegion, [[&str; 2]; 3]); 2] = [
    (
        OkxRegion::Okx,
        [
            [
                "wss://ws.okx.com/ws/v5/public",
                "wss://wspap.okx.com/ws/v5/public",
            ],
            [
                "wss://ws.okx.com/ws/v5/private",
                "wss://wspap.okx.com/ws/v5/private",
            ],
            [
                "wss://ws.okx.com/ws/v5/business",
                "wss://wspap.okx.com/ws/v5/business",
            ],
        ],
    ),
    (
        OkxRegion::Okxeu,
        [
            [
                "wss://wseea.okx.com/ws/v5/public",
                "wss://wseeapap.okx.com/ws/v5/public",
            ],
            [
                "wss://wseea.okx.com/ws/v5/private",
                "wss://wseeapap.okx.com/ws/v5/private",
            ],
            [
                "wss://wseea.okx.com/ws/v5/business",
                "wss://wseeapap.okx.com/ws/v5/business",
            ],
        ],
    ),
];
pub const ENDPOINTS: [(&str, &str); 28] = [
    ("ACCOUNT", "/api/v5/account/config"),
    ("ACCOUNT_INSTRUMENTS", "/api/v5/account/instruments"),
    ("PARTNER", "/api/v5/users/partner/if-rebate"),
    ("MARKETS", "/api/v5/public/instruments"),
    ("TICKERS", "/api/v5/market/tickers"),
    ("BALANCE", "/api/v5/account/account-position-risk"),
    ("POSITIONS", "/api/v5/account/positions"),
    ("KLINE", "/api/v5/market/candles"),
    ("KLINE_HISTORY", "/api/v5/market/history-candles"),
    ("ORDERS_HISTORY", "/api/v5/trade/orders-history"),
    ("FILLS", "/api/v5/trade/fills"),
    ("FILLS_HISTORY", "/api/v5/trade/fills-history"),
    ("POSITIONS_HISTORY", "/api/v5/account/positions-history"),
    ("BILLS", "/api/v5/account/bills"),
    ("BILLS_ARCHIVE", "/api/v5/account/bills-archive"),
    ("UNFILLED_ORDERS", "/api/v5/trade/orders-pending"),
    ("ORDER_DETAIL", "/api/v5/trade/order"),
    ("UNFILLED_ALGO_ORDERS", "/api/v5/trade/orders-algo-pending"),
    ("CANCEL_ORDERS", "/api/v5/trade/cancel-batch-orders"),
    ("CANCEL_ALGO_ORDERS", "/api/v5/trade/cancel-algos"),
    ("UPDATE_ALGO_ORDER", "/api/v5/trade/amend-algos"),
    ("PLACE_ORDERS", "/api/v5/trade/batch-orders"),
    ("PLACE_ALGO_ORDER", "/api/v5/trade/order-algo"),
    ("SET_LEVERAGE", "/api/v5/account/set-leverage"),
    ("LEVERAGE", "/api/v5/account/leverage-info"),
    ("SET_POSITION_MODE", "/api/v5/account/set-position-mode"),
    ("ACCOUNT_CONFIG", "/api/v5/account/config"),
    ("UPDATE_ORDER", "/api/v5/trade/amend-order"),
];
pub const GRAFANA_ENDPOINTS_EVERY: [&str; 9] = [
    "/api/v5/trade/cancel-batch-orders",
    "/api/v5/trade/cancel-algos",
    "/api/v5/trade/amend-algos",
    "/api/v5/trade/batch-orders",
    "/api/v5/trade/order-algo",
    "/api/v5/trade/amend-order",
    "/api/v5/account/set-leverage",
    "/api/v5/account/leverage-info",
    "/api/v5/account/set-position-mode",
];
pub const GRAFANA_ENDPOINTS_PERIODIC: [&str; 4] = [
    "/api/v5/account/account-position-risk",
    "/api/v5/account/positions",
    "/api/v5/trade/orders-pending",
    "/api/v5/trade/orders-algo-pending",
];
pub const INTERVAL: [(&str, &str); 13] = [
    ("1m", "1m"),
    ("3m", "3m"),
    ("5m", "5m"),
    ("15m", "15m"),
    ("30m", "30m"),
    ("1h", "1H"),
    ("2h", "2H"),
    ("4h", "4H"),
    ("6h", "6H"),
    ("12h", "12H"),
    ("1d", "1D"),
    ("1w", "1W"),
    ("1M", "1Mutc"),
];
pub const ORDER_STATUS: [(&str, OrderStatus); 2] = [
    ("live", OrderStatus::Open),
    ("partially_filled", OrderStatus::Open),
];
pub const ORDER_TYPE: [(&str, OrderType); 6] = [
    ("market", OrderType::Market),
    ("limit", OrderType::Limit),
    ("post_only", OrderType::Limit),
    ("fok", OrderType::Limit),
    ("ioc", OrderType::Limit),
    ("optimal_limit_ioc", OrderType::Market),
];
pub const ORDER_SIDE: [(&str, OrderSide); 2] = [("buy", OrderSide::Buy), ("sell", OrderSide::Sell)];
pub const POSITION_SIDE: [(&str, PositionSide); 2] =
    [("long", PositionSide::Long), ("short", PositionSide::Short)];
pub const REVERSE_ORDER_TYPE: [(OrderType, &str); 6] = [
    (OrderType::Market, "market"),
    (OrderType::Limit, "limit"),
    (OrderType::Conditional, "trigger"),
    (OrderType::StopLoss, "trigger"),
    (OrderType::TakeProfit, "trigger"),
    (OrderType::TrailingStopLoss, "move_order_stop"),
];

#[cfg(test)]
mod accepted_inventory {
    use super::*;
    #[derive(Default)]
    struct Inventory(Option<&'static [&'static str]>);
    impl<'de> serde::Deserializer<'de> for &mut Inventory {
        type Error = serde::de::value::Error;
        fn deserialize_enum<V: serde::de::Visitor<'de>>(
            self,
            _: &'static str,
            variants: &'static [&'static str],
            _: V,
        ) -> Result<V::Value, Self::Error> {
            self.0 = Some(variants);
            Err(serde::de::Error::custom("inventory captured"))
        }
        fn deserialize_any<V: serde::de::Visitor<'de>>(
            self,
            _: V,
        ) -> Result<V::Value, Self::Error> {
            Err(serde::de::Error::custom("expected enum inventory"))
        }
        serde::forward_to_deserialize_any! {
            bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf
            option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any
        }
    }
    fn prove_private_wire<T: for<'de> Deserialize<'de> + Serialize>(
        name: &str,
        literals: &[&str],
        native_variant: impl Fn(&T) -> &'static str,
    ) {
        for &literal in literals {
            let value = serde_json::json!(literal);
            let text = serde_json::to_string(&value).unwrap();
            let decoded = [
                serde_json::from_value::<T>(value.clone()).unwrap(),
                T::deserialize(&value).unwrap(),
                <T as Deserialize>::deserialize(value.clone()).unwrap(),
                serde_json::from_str::<T>(&text).unwrap(),
            ];
            for decoded in decoded {
                assert_eq!(native_variant(&decoded), literal, "{name} native variant");
                assert_eq!(serde_json::to_value(&decoded).unwrap(), value);
            }
            let tagged = serde_json::json!({literal: null});
            assert!(
                serde_json::from_value::<T>(tagged.clone()).is_err(),
                "{name} tagged owned shape"
            );
            assert!(
                T::deserialize(&tagged).is_err(),
                "{name} tagged borrowed shape"
            );
            assert!(
                <T as Deserialize>::deserialize(tagged.clone()).is_err(),
                "{name} tagged qualified shape"
            );
            assert!(
                serde_json::from_str::<T>(&tagged.to_string()).is_err(),
                "{name} tagged text shape"
            );
            for invalid in [
                serde_json::json!([]),
                serde_json::json!([literal]),
                serde_json::json!(null),
                serde_json::json!(true),
                serde_json::json!(1),
                serde_json::json!(format!(" {literal}")),
                serde_json::json!(format!("{literal} ")),
                serde_json::json!(format!("\u{feff}{literal}")),
                serde_json::json!(format!("{literal}__unknown")),
            ] {
                assert!(
                    T::deserialize(&invalid).is_err(),
                    "{name} invalid spelling/shape"
                );
            }
            for case in [literal.to_uppercase(), literal.to_lowercase()] {
                if !literals.contains(&case.as_str()) {
                    assert!(T::deserialize(serde_json::json!(case)).is_err());
                }
            }
        }
    }
    #[test]
    fn private_wire_region_rejects_tagged_maps() {
        let native = |value: &RegionWire| match value {
            RegionWire::Okx => "okx",
            RegionWire::Okxeu => "okxeu",
        };
        prove_private_wire::<RegionWire>("RegionWire", &["okx", "okxeu"], native);
        assert_eq!(native(&RegionWire::from(OkxRegion::Okx)), "okx");
        assert_eq!(native(&RegionWire::from(OkxRegion::Okxeu)), "okxeu");
    }
    #[test]
    fn private_wire_instrument_rejects_tagged_maps() {
        let native = |value: &InstrumentWire| match value {
            InstrumentWire::Swap => "SWAP",
            InstrumentWire::Futures => "FUTURES",
        };
        prove_private_wire::<InstrumentWire>("InstrumentWire", &["SWAP", "FUTURES"], native);
        assert_eq!(
            native(&InstrumentWire::from(OkxInstrumentType::Swap)),
            "SWAP"
        );
        assert_eq!(
            native(&InstrumentWire::from(OkxInstrumentType::Futures)),
            "FUTURES"
        );
    }
    #[test]
    #[ignore = "requires live TypeScript oracle; mandatory in CI"]
    fn derived_accepted_sets_match_typescript() {
        let oracle = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/tables-oracle.mjs"
            ))
            .output()
            .unwrap();
        assert!(
            oracle.status.success(),
            "{}",
            String::from_utf8_lossy(&oracle.stderr)
        );
        let expected: serde_json::Value = serde_json::from_slice(&oracle.stdout).unwrap();
        let mut inventory = Inventory::default();
        let _ = RegionWireSchema::deserialize(&mut inventory);
        assert_eq!(
            serde_json::json!(inventory.0.unwrap()),
            expected["OkxRegion"]
        );
        let _ = InstrumentWireSchema::deserialize(&mut inventory);
        assert_eq!(
            serde_json::json!(inventory.0.unwrap()),
            expected["OkxInstrumentType"]
        );
    }
}
