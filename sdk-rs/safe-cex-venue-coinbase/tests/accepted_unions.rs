use safe_cex_venue_coinbase::{starbase_cutover, types};
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{collections::BTreeSet, process::Command, sync::OnceLock};
#[derive(Debug)]
struct InventoryError {
    variants: Option<&'static [&'static str]>,
}
impl std::fmt::Display for InventoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("enum inventory probe")
    }
}
impl std::error::Error for InventoryError {}
impl serde::de::Error for InventoryError {
    fn custom<T: std::fmt::Display>(_: T) -> Self {
        Self { variants: None }
    }
    fn unknown_variant(_value: &str, variants: &'static [&'static str]) -> Self {
        Self {
            variants: Some(variants),
        }
    }
}
#[derive(Default)]
struct EnumInventory(Option<&'static [&'static str]>);
impl<'de> serde::Deserializer<'de> for &mut EnumInventory {
    type Error = InventoryError;
    fn deserialize_enum<V: serde::de::Visitor<'de>>(
        self,
        _name: &'static str,
        variants: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        self.0 = Some(variants);
        Err(serde::de::Error::custom("captured accepted variants"))
    }
    fn deserialize_any<V: serde::de::Visitor<'de>>(
        self,
        _visitor: V,
    ) -> Result<V::Value, Self::Error> {
        _visitor.visit_string("__COINBASE_INVALID_INVENTORY_PROBE__".to_owned())
    }
    serde::forward_to_deserialize_any! {bool i8 i16 i32 i64 u8 u16 u32 u64 f32 f64 char str string bytes byte_buf option unit unit_struct newtype_struct seq tuple tuple_struct map struct identifier ignored_any}
}
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/unions.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
fn audit<T: DeserializeOwned>(expected: &Value, name: &str) {
    let expected = expected.as_array().expect("live TS enum exists");
    let mut metadata = EnumInventory::default();
    let result = T::deserialize(&mut metadata);
    let accepted = metadata
        .0
        .or_else(|| result.err().and_then(|error| error.variants));
    if let Some(accepted) = accepted {
        let actual: BTreeSet<_> = accepted.iter().copied().collect();
        let expected: BTreeSet<_> = expected
            .iter()
            .map(|v| v.as_str().expect("string enum"))
            .collect();
        assert_eq!(
            actual, expected,
            "{name}: actual Serde accepted set versus TS"
        );
    } else {
        assert_eq!(
            name, "CoinbaseStarbaseFalse",
            "{name}: missing derived enum inventory"
        );
    }
    let mut probes = vec![
        json!(null),
        json!(true),
        json!(false),
        json!(0),
        json!(1),
        json!({}),
        json!([]),
        json!(""),
        json!("constructor"),
        json!("__proto__"),
        json!("unknown"),
    ];
    for v in expected {
        probes.push(v.clone());
        if let Some(tag) = v.as_str() {
            probes.push(json!({tag:null}));
        }
        if let Some(s) = v.as_str() {
            probes.extend([json!(s.to_uppercase()), json!(s.to_lowercase())]);
            for padding in [" ", "\n", "\u{feff}", "\u{85}", "_alias"] {
                probes.extend([
                    json!(format!("{padding}{s}")),
                    json!(format!("{s}{padding}")),
                ]);
            }
        }
    }
    for probe in probes {
        assert_eq!(
            serde_json::from_value::<T>(probe.clone()).is_ok(),
            expected.contains(&probe),
            "{name}: acceptance of {probe}"
        );
    }
}
fn declared_enums(source: &str) -> BTreeSet<String> {
    syn::parse_file(source)
        .unwrap()
        .items
        .into_iter()
        .filter_map(|item| {
            if let syn::Item::Enum(e) = item {
                Some(e.ident.to_string())
            } else {
                None
            }
        })
        .collect()
}

#[test]
fn legacy_serde_accepted_union_inventory() {
    let mut visited = BTreeSet::new();
    audit::<types::ListPositionsIntxResponsePositionsItemPositionSide>(
        &oracle()["legacy"]["ListPositionsIntxResponsePositionsItemPositionSide"],
        "ListPositionsIntxResponsePositionsItemPositionSide",
    );
    visited.insert("ListPositionsIntxResponsePositionsItemPositionSide".to_owned());
    audit::<types::ListPositionsCfmResponsePositionsItemSide>(
        &oracle()["legacy"]["ListPositionsCfmResponsePositionsItemSide"],
        "ListPositionsCfmResponsePositionsItemSide",
    );
    visited.insert("ListPositionsCfmResponsePositionsItemSide".to_owned());
    audit::<types::CandleEventType>(&oracle()["legacy"]["CandleEventType"], "CandleEventType");
    visited.insert("CandleEventType".to_owned());
    audit::<types::TradeEventType>(&oracle()["legacy"]["TradeEventType"], "TradeEventType");
    visited.insert("TradeEventType".to_owned());
    audit::<types::TradeEventTradesItemSide>(
        &oracle()["legacy"]["TradeEventTradesItemSide"],
        "TradeEventTradesItemSide",
    );
    visited.insert("TradeEventTradesItemSide".to_owned());
    audit::<types::TickerEventType>(&oracle()["legacy"]["TickerEventType"], "TickerEventType");
    visited.insert("TickerEventType".to_owned());
    audit::<types::TickerEventTickersItemType>(
        &oracle()["legacy"]["TickerEventTickersItemType"],
        "TickerEventTickersItemType",
    );
    visited.insert("TickerEventTickersItemType".to_owned());
    audit::<types::Level2EventType>(&oracle()["legacy"]["Level2EventType"], "Level2EventType");
    visited.insert("Level2EventType".to_owned());
    audit::<types::Level2EventUpdatesItemSide>(
        &oracle()["legacy"]["Level2EventUpdatesItemSide"],
        "Level2EventUpdatesItemSide",
    );
    visited.insert("Level2EventUpdatesItemSide".to_owned());
    audit::<types::OrderEventType>(&oracle()["legacy"]["OrderEventType"], "OrderEventType");
    visited.insert("OrderEventType".to_owned());
    audit::<types::OrderEventOrdersItemStatus>(
        &oracle()["legacy"]["OrderEventOrdersItemStatus"],
        "OrderEventOrdersItemStatus",
    );
    visited.insert("OrderEventOrdersItemStatus".to_owned());
    audit::<types::OrderEventOrdersItemOrderSide>(
        &oracle()["legacy"]["OrderEventOrdersItemOrderSide"],
        "OrderEventOrdersItemOrderSide",
    );
    visited.insert("OrderEventOrdersItemOrderSide".to_owned());
    audit::<types::OrderEventOrdersItemOrderType>(
        &oracle()["legacy"]["OrderEventOrdersItemOrderType"],
        "OrderEventOrdersItemOrderType",
    );
    visited.insert("OrderEventOrdersItemOrderType".to_owned());
    audit::<types::ListOrdersResponseOrdersItemSide>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemSide"],
        "ListOrdersResponseOrdersItemSide",
    );
    visited.insert("ListOrdersResponseOrdersItemSide".to_owned());
    audit::<types::ListOrdersResponseOrdersItemStatus>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemStatus"],
        "ListOrdersResponseOrdersItemStatus",
    );
    visited.insert("ListOrdersResponseOrdersItemStatus".to_owned());
    audit::<types::ListOrdersResponseOrdersItemOrderType>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemOrderType"],
        "ListOrdersResponseOrdersItemOrderType",
    );
    visited.insert("ListOrdersResponseOrdersItemOrderType".to_owned());
    audit::<types::StopDirection>(&oracle()["legacy"]["StopDirection"], "StopDirection");
    visited.insert("StopDirection".to_owned());
    audit::<types::MarginType>(&oracle()["legacy"]["MarginType"], "MarginType");
    visited.insert("MarginType".to_owned());
    audit::<types::PlaceOrderRequestSide>(
        &oracle()["legacy"]["PlaceOrderRequestSide"],
        "PlaceOrderRequestSide",
    );
    visited.insert("PlaceOrderRequestSide".to_owned());
    audit::<types::PortfolioBreakdownResponseBreakdownPerpPositionsItemSide>(
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownPerpPositionsItemSide"],
        "PortfolioBreakdownResponseBreakdownPerpPositionsItemSide",
    );
    visited.insert("PortfolioBreakdownResponseBreakdownPerpPositionsItemSide".to_owned());
    audit::<types::PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide>(
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide"],
        "PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide",
    );
    visited.insert("PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide".to_owned());
    assert_eq!(
        visited,
        declared_enums(include_str!("../src/types.rs")),
        "all Rust enums audited"
    );
    assert_eq!(
        visited,
        oracle()["legacy"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
        "all TS unions audited"
    );
}

#[test]
fn cutover_serde_accepted_union_inventory() {
    let mut visited = BTreeSet::new();
    audit::<starbase_cutover::CoinbaseStarbaseImplementationStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseImplementationStatus"],
        "CoinbaseStarbaseImplementationStatus",
    );
    visited.insert("CoinbaseStarbaseImplementationStatus".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseVerificationStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseVerificationStatus"],
        "CoinbaseStarbaseVerificationStatus",
    );
    visited.insert("CoinbaseStarbaseVerificationStatus".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowBlocker"],
        "CoinbaseStarbaseHttpShadowBlocker",
    );
    visited.insert("CoinbaseStarbaseHttpShadowBlocker".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseWebsocketBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseWebsocketBlocker"],
        "CoinbaseStarbaseWebsocketBlocker",
    );
    visited.insert("CoinbaseStarbaseWebsocketBlocker".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverBlocker"],
        "CoinbaseStarbaseCutoverBlocker",
    );
    visited.insert("CoinbaseStarbaseCutoverBlocker".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseTestProductClassification>(
        &oracle()["cutover"]["CoinbaseStarbaseTestProductClassification"],
        "CoinbaseStarbaseTestProductClassification",
    );
    visited.insert("CoinbaseStarbaseTestProductClassification".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanAuthentication>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanAuthentication"],
        "CoinbaseStarbaseHttpShadowReadinessPlanAuthentication",
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlanAuthentication".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads"],
        "CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads",
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPublicReads>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPublicReads"],
        "CoinbaseStarbaseHttpShadowReadinessPlanPublicReads",
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlanPublicReads".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanTransport>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanTransport"],
        "CoinbaseStarbaseHttpShadowReadinessPlanTransport",
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlanTransport".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanWrites>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanWrites"],
        "CoinbaseStarbaseHttpShadowReadinessPlanWrites",
    );
    visited.insert("CoinbaseStarbaseHttpShadowReadinessPlanWrites".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingRequired>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingRequired"],
        "CoinbaseStarbaseCutoverPlanProductBindingRequired",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanProductBindingRequired".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingStatus"],
        "CoinbaseStarbaseCutoverPlanProductBindingStatus",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanProductBindingStatus".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads"],
        "CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePublicReads>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePublicReads"],
        "CoinbaseStarbaseCutoverPlanAuthoritativePublicReads",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanAuthoritativePublicReads".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativeWrites>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativeWrites"],
        "CoinbaseStarbaseCutoverPlanAuthoritativeWrites",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanAuthoritativeWrites".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseFalse>(
        &oracle()["cutover"]["CoinbaseStarbaseFalse"],
        "CoinbaseStarbaseFalse",
    );
    visited.insert("CoinbaseStarbaseFalse".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem"],
        "CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications"],
        "CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions"],
        "CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions".to_owned());
    audit::<starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketSession>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketSession"],
        "CoinbaseStarbaseCutoverPlanWebsocketSession",
    );
    visited.insert("CoinbaseStarbaseCutoverPlanWebsocketSession".to_owned());
    assert_eq!(
        visited,
        declared_enums(include_str!("../src/starbase_cutover.rs")),
        "all Rust enums audited"
    );
    assert_eq!(
        visited,
        oracle()["cutover"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect(),
        "all TS unions audited"
    );
}

fn accepts_tagged_object<T: DeserializeOwned>(values: &Value) -> bool {
    values
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .any(|tag| serde_json::from_value::<T>(json!({tag:null})).is_ok())
}
#[test]
fn no_literal_union_accepts_externally_tagged_objects() {
    let mut widened = Vec::new();
    if accepts_tagged_object::<types::ListPositionsIntxResponsePositionsItemPositionSide>(
        &oracle()["legacy"]["ListPositionsIntxResponsePositionsItemPositionSide"],
    ) {
        widened.push("ListPositionsIntxResponsePositionsItemPositionSide");
    }
    if accepts_tagged_object::<types::ListPositionsCfmResponsePositionsItemSide>(
        &oracle()["legacy"]["ListPositionsCfmResponsePositionsItemSide"],
    ) {
        widened.push("ListPositionsCfmResponsePositionsItemSide");
    }
    if accepts_tagged_object::<types::CandleEventType>(&oracle()["legacy"]["CandleEventType"]) {
        widened.push("CandleEventType");
    }
    if accepts_tagged_object::<types::TradeEventType>(&oracle()["legacy"]["TradeEventType"]) {
        widened.push("TradeEventType");
    }
    if accepts_tagged_object::<types::TradeEventTradesItemSide>(
        &oracle()["legacy"]["TradeEventTradesItemSide"],
    ) {
        widened.push("TradeEventTradesItemSide");
    }
    if accepts_tagged_object::<types::TickerEventType>(&oracle()["legacy"]["TickerEventType"]) {
        widened.push("TickerEventType");
    }
    if accepts_tagged_object::<types::TickerEventTickersItemType>(
        &oracle()["legacy"]["TickerEventTickersItemType"],
    ) {
        widened.push("TickerEventTickersItemType");
    }
    if accepts_tagged_object::<types::Level2EventType>(&oracle()["legacy"]["Level2EventType"]) {
        widened.push("Level2EventType");
    }
    if accepts_tagged_object::<types::Level2EventUpdatesItemSide>(
        &oracle()["legacy"]["Level2EventUpdatesItemSide"],
    ) {
        widened.push("Level2EventUpdatesItemSide");
    }
    if accepts_tagged_object::<types::OrderEventType>(&oracle()["legacy"]["OrderEventType"]) {
        widened.push("OrderEventType");
    }
    if accepts_tagged_object::<types::OrderEventOrdersItemStatus>(
        &oracle()["legacy"]["OrderEventOrdersItemStatus"],
    ) {
        widened.push("OrderEventOrdersItemStatus");
    }
    if accepts_tagged_object::<types::OrderEventOrdersItemOrderSide>(
        &oracle()["legacy"]["OrderEventOrdersItemOrderSide"],
    ) {
        widened.push("OrderEventOrdersItemOrderSide");
    }
    if accepts_tagged_object::<types::OrderEventOrdersItemOrderType>(
        &oracle()["legacy"]["OrderEventOrdersItemOrderType"],
    ) {
        widened.push("OrderEventOrdersItemOrderType");
    }
    if accepts_tagged_object::<types::ListOrdersResponseOrdersItemSide>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemSide"],
    ) {
        widened.push("ListOrdersResponseOrdersItemSide");
    }
    if accepts_tagged_object::<types::ListOrdersResponseOrdersItemStatus>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemStatus"],
    ) {
        widened.push("ListOrdersResponseOrdersItemStatus");
    }
    if accepts_tagged_object::<types::ListOrdersResponseOrdersItemOrderType>(
        &oracle()["legacy"]["ListOrdersResponseOrdersItemOrderType"],
    ) {
        widened.push("ListOrdersResponseOrdersItemOrderType");
    }
    if accepts_tagged_object::<types::StopDirection>(&oracle()["legacy"]["StopDirection"]) {
        widened.push("StopDirection");
    }
    if accepts_tagged_object::<types::MarginType>(&oracle()["legacy"]["MarginType"]) {
        widened.push("MarginType");
    }
    if accepts_tagged_object::<types::PlaceOrderRequestSide>(
        &oracle()["legacy"]["PlaceOrderRequestSide"],
    ) {
        widened.push("PlaceOrderRequestSide");
    }
    if accepts_tagged_object::<types::PortfolioBreakdownResponseBreakdownPerpPositionsItemSide>(
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownPerpPositionsItemSide"],
    ) {
        widened.push("PortfolioBreakdownResponseBreakdownPerpPositionsItemSide");
    }
    if accepts_tagged_object::<types::PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide>(
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide"],
    ) {
        widened.push("PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseImplementationStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseImplementationStatus"],
    ) {
        widened.push("CoinbaseStarbaseImplementationStatus");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseVerificationStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseVerificationStatus"],
    ) {
        widened.push("CoinbaseStarbaseVerificationStatus");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseHttpShadowBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowBlocker"],
    ) {
        widened.push("CoinbaseStarbaseHttpShadowBlocker");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseWebsocketBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseWebsocketBlocker"],
    ) {
        widened.push("CoinbaseStarbaseWebsocketBlocker");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverBlocker>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverBlocker"],
    ) {
        widened.push("CoinbaseStarbaseCutoverBlocker");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseTestProductClassification>(
        &oracle()["cutover"]["CoinbaseStarbaseTestProductClassification"],
    ) {
        widened.push("CoinbaseStarbaseTestProductClassification");
    }
    if accepts_tagged_object::<
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanAuthentication,
    >(&oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanAuthentication"])
    {
        widened.push("CoinbaseStarbaseHttpShadowReadinessPlanAuthentication");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads"],
    ) {
        widened.push("CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPublicReads>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPublicReads"],
    ) {
        widened.push("CoinbaseStarbaseHttpShadowReadinessPlanPublicReads");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanTransport>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanTransport"],
    ) {
        widened.push("CoinbaseStarbaseHttpShadowReadinessPlanTransport");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanWrites>(
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanWrites"],
    ) {
        widened.push("CoinbaseStarbaseHttpShadowReadinessPlanWrites");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingRequired>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingRequired"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanProductBindingRequired");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingStatus>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingStatus"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanProductBindingStatus");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePublicReads>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePublicReads"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanAuthoritativePublicReads");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativeWrites>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativeWrites"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanAuthoritativeWrites");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseFalse>(
        &oracle()["cutover"]["CoinbaseStarbaseFalse"],
    ) {
        widened.push("CoinbaseStarbaseFalse");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem");
    }
    if accepts_tagged_object::<
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications,
    >(&oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications"])
    {
        widened.push("CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications");
    }
    if accepts_tagged_object::<
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions,
    >(&oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions"])
    {
        widened.push("CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions");
    }
    if accepts_tagged_object::<starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketSession>(
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketSession"],
    ) {
        widened.push("CoinbaseStarbaseCutoverPlanWebsocketSession");
    }
    assert!(
        widened.is_empty(),
        "{} widened unions: {widened:?}",
        widened.len()
    );
}

#[test]
fn qualified_public_decoders_reject_tagged_objects() {
    macro_rules! probe {
        ($ty:path, $values:expr) => {
            for tag in $values.as_array().unwrap().iter().filter_map(Value::as_str) {
                // Concrete qualified calls catch an inherent decoder shadowing the trait.
                assert!(<$ty>::deserialize(json!({tag:null})).is_err(), "{} accepted tagged {tag}", stringify!($ty));
            }
        };
    }
    probe!(
        types::ListPositionsIntxResponsePositionsItemPositionSide,
        &oracle()["legacy"]["ListPositionsIntxResponsePositionsItemPositionSide"]
    );
    probe!(
        types::ListPositionsCfmResponsePositionsItemSide,
        &oracle()["legacy"]["ListPositionsCfmResponsePositionsItemSide"]
    );
    probe!(
        types::CandleEventType,
        &oracle()["legacy"]["CandleEventType"]
    );
    probe!(types::TradeEventType, &oracle()["legacy"]["TradeEventType"]);
    probe!(
        types::TradeEventTradesItemSide,
        &oracle()["legacy"]["TradeEventTradesItemSide"]
    );
    probe!(
        types::TickerEventType,
        &oracle()["legacy"]["TickerEventType"]
    );
    probe!(
        types::TickerEventTickersItemType,
        &oracle()["legacy"]["TickerEventTickersItemType"]
    );
    probe!(
        types::Level2EventType,
        &oracle()["legacy"]["Level2EventType"]
    );
    probe!(
        types::Level2EventUpdatesItemSide,
        &oracle()["legacy"]["Level2EventUpdatesItemSide"]
    );
    probe!(types::OrderEventType, &oracle()["legacy"]["OrderEventType"]);
    probe!(
        types::OrderEventOrdersItemStatus,
        &oracle()["legacy"]["OrderEventOrdersItemStatus"]
    );
    probe!(
        types::OrderEventOrdersItemOrderSide,
        &oracle()["legacy"]["OrderEventOrdersItemOrderSide"]
    );
    probe!(
        types::OrderEventOrdersItemOrderType,
        &oracle()["legacy"]["OrderEventOrdersItemOrderType"]
    );
    probe!(
        types::ListOrdersResponseOrdersItemSide,
        &oracle()["legacy"]["ListOrdersResponseOrdersItemSide"]
    );
    probe!(
        types::ListOrdersResponseOrdersItemStatus,
        &oracle()["legacy"]["ListOrdersResponseOrdersItemStatus"]
    );
    probe!(
        types::ListOrdersResponseOrdersItemOrderType,
        &oracle()["legacy"]["ListOrdersResponseOrdersItemOrderType"]
    );
    probe!(types::StopDirection, &oracle()["legacy"]["StopDirection"]);
    probe!(types::MarginType, &oracle()["legacy"]["MarginType"]);
    probe!(
        types::PlaceOrderRequestSide,
        &oracle()["legacy"]["PlaceOrderRequestSide"]
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownPerpPositionsItemSide,
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownPerpPositionsItemSide"]
    );
    probe!(
        types::PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide,
        &oracle()["legacy"]["PortfolioBreakdownResponseBreakdownFuturesPositionsItemSide"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseImplementationStatus,
        &oracle()["cutover"]["CoinbaseStarbaseImplementationStatus"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseVerificationStatus,
        &oracle()["cutover"]["CoinbaseStarbaseVerificationStatus"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowBlocker,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowBlocker"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseWebsocketBlocker,
        &oracle()["cutover"]["CoinbaseStarbaseWebsocketBlocker"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverBlocker,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverBlocker"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseTestProductClassification,
        &oracle()["cutover"]["CoinbaseStarbaseTestProductClassification"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanAuthentication,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanAuthentication"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPrivateReads"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanPublicReads,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanPublicReads"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanTransport,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanTransport"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseHttpShadowReadinessPlanWrites,
        &oracle()["cutover"]["CoinbaseStarbaseHttpShadowReadinessPlanWrites"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingRequired,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingRequired"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanProductBindingStatus,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanProductBindingStatus"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePrivateReads"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativePublicReads,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativePublicReads"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanAuthoritativeWrites,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanAuthoritativeWrites"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseFalse,
        &oracle()["cutover"]["CoinbaseStarbaseFalse"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanHttpShadowBlockersItem"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPrivateNotifications"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketPublicSubscriptions"]
    );
    probe!(
        starbase_cutover::CoinbaseStarbaseCutoverPlanWebsocketSession,
        &oracle()["cutover"]["CoinbaseStarbaseCutoverPlanWebsocketSession"]
    );
}
