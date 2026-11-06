//! fastAssetCtxs carries every HIP-3 dex's mids and marks, which is why no per-dex
//! allMids is subscribed: a frame naming a dex coin moves that market's prices.
mod support;

use base64::Engine as _;
use safe_cex_engine::Event;

#[test]
fn a_fast_asset_contexts_frame_moves_a_hip3_markets_last_and_mark() {
    let (mut e, socket, initial) =
        support::ready_with_policy("disabled", safe_cex_engine::PublicFeedPolicy::Direct);
    assert!(
        !initial
            .iter()
            .any(|text| text.contains("\"type\":\"allMids\"") && text.contains("\"dex\"")),
        "no per-dex allMids"
    );
    let contexts = br#"{"xyz:FOO":{"markPx":"61","midPx":"60.5"}}"#;
    let data = base64::engine::general_purpose::STANDARD
        .encode(miniz_oxide::deflate::compress_to_vec(contexts, 6));
    let text = format!(r#"{{"channel":"fastAssetCtxs","data":"{data}"}}"#);
    e.handle(
        2000.,
        Event::WsMessage {
            socket,
            text: &text,
        },
    );
    let ticker = e.store().ticker("XYZ|FOO-PERP").expect("HIP-3 ticker");
    assert_eq!((ticker.last, ticker.mark), (60.5, 61.));
}
