// HTTP completion publishes diagnostics even when a stale/failed read publishes
// no data. Pin the entire emitted frame to STATUS plus the exact counter delta.
pub fn assert_request_stats_frame(before: &[u8], frame: Option<&[u8]>, now: f64, delta: [u32; 5]) {
    if delta == [0; 5] {
        assert!(
            frame.is_none(),
            "no physical outcome must remain frame-inert"
        );
        return;
    }
    use safe_cex_engine::wire::{section, sections};
    let mut expected = before[..16].to_vec();
    expected[6..8].copy_from_slice(&2u16.to_le_bytes());
    expected[8..16].copy_from_slice(&now.to_le_bytes());
    let mut at = 16;
    let mut kinds = Vec::new();
    for (kind, payload) in sections(before) {
        let end = at + 8 + payload.len();
        if matches!(kind, section::STATUS | section::REQUEST_STATS) {
            kinds.push(kind);
            let mut bytes = before[at..end].to_vec();
            if kind == section::REQUEST_STATS {
                for (index, increment) in delta.into_iter().enumerate() {
                    let at = 8 + index * 4;
                    let value = u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                    bytes[at..at + 4].copy_from_slice(&(value + increment).to_le_bytes());
                }
            }
            expected.extend_from_slice(&bytes);
        }
        at = end;
    }
    assert_eq!(kinds, [section::STATUS, section::REQUEST_STATS]);
    assert_eq!(
        frame.expect("physical outcome must publish request stats"),
        expected
    );
}
