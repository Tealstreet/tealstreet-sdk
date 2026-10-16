// Source-derived string boundary inventory.
fn capture_null_type_strings(name:&str,raw:&str)->Result<std::collections::BTreeMap<String,Vec<u16>>,serde_json::Error>{
 match name {
 "AssetCtx"=>capture_strings::<safe_cex_venue_hyperliquid::types::AssetCtx>(raw),
 "SpotAssetCtx"=>capture_strings::<safe_cex_venue_hyperliquid::types::SpotAssetCtx>(raw),
 _=>panic!("unrecognized source string-bearing numeric type {name}"),
 }
}
