fn dispatch_enum(name:&str,raw:&str)->serde_json::Value {match name {
"HyperliquidUserAbstractionMode"=>roundtrip::<safe_cex_venue_hyperliquid::types::HyperliquidUserAbstractionMode>(raw),
"Tif"=>roundtrip::<safe_cex_venue_hyperliquid::types::Tif>(raw),
"Tpsl"=>roundtrip::<safe_cex_venue_hyperliquid::types::Tpsl>(raw),
"LimitOrderType"=>roundtrip::<safe_cex_venue_hyperliquid::types::LimitOrderType>(raw),
"TriggerOrderType"=>roundtrip::<safe_cex_venue_hyperliquid::types::TriggerOrderType>(raw),
"Grouping"=>roundtrip::<safe_cex_venue_hyperliquid::types::Grouping>(raw),
"OrderType"=>roundtrip::<safe_cex_venue_hyperliquid::types::OrderType>(raw),
"WsUserNonFundingLedgerUpdate"=>roundtrip::<safe_cex_venue_hyperliquid::types::WsUserNonFundingLedgerUpdate>(raw),
"WsUserNonFundingLedgerUpdates"=>roundtrip::<safe_cex_venue_hyperliquid::types::WsUserNonFundingLedgerUpdates>(raw),
_=>panic!("unknown identity root {name}")}}
