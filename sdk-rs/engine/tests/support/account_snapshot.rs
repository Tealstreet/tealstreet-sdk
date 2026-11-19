// Rule 5 / #3640: reads preserve every financial/account byte while
// section 35 independently records the final physical HTTP outcome.
pub fn assert_unchanged_account(before: &[u8], after: &[u8], delta: [u32; 5]) {
    assert_eq!(&after[..16], &before[..16]);
    let sections = |frame: &[u8]| {
        let mut at = 16;
        safe_cex_engine::wire::sections(frame)
            .map(|(kind, payload)| {
                let end = at + 8 + payload.len();
                let bytes = frame[at..end].to_vec();
                at = end;
                (kind, bytes)
            })
            .collect::<Vec<_>>()
    };
    let before = sections(before);
    let after = sections(after);
    assert_eq!(after.len(), before.len());
    let mut stats_seen = false;
    for ((before_kind, mut expected), (after_kind, actual)) in before.into_iter().zip(after) {
        assert_eq!(after_kind, before_kind);
        if before_kind == safe_cex_engine::wire::section::REQUEST_STATS {
            stats_seen = true;
            for (index, increment) in delta.into_iter().enumerate() {
                let at = 8 + index * 4;
                let previous = u32::from_le_bytes(expected[at..at + 4].try_into().unwrap());
                expected[at..at + 4].copy_from_slice(&(previous + increment).to_le_bytes());
            }
        }
        assert_eq!(actual, expected, "account section {before_kind}");
    }
    assert!(
        stats_seen,
        "response snapshot must carry request statistics"
    );
}
