use safe_cex_venue_okx::price_load::*;
use std::{
    collections::HashSet,
    io::Write,
    process::{Command, Stdio},
};

#[test]
fn price_load_native_utf16_state_before_source() {
    let ids = vec![
        "BTC-USDT".encode_utf16().collect::<Vec<_>>(),
        vec![],
        vec![0xd800],
        vec![0xdc00],
        vec![0xd83d, 0xde42],
    ];
    let mut marks = HashSet::new();
    let mut indexes = HashSet::new();
    let mut native = vec![];
    for (index, id) in ids.iter().enumerate() {
        let mut states = vec![];
        let before = [
            has_mark_price_loaded(&marks, id),
            has_index_price_loaded(&indexes, id),
        ];
        assert_eq!(
            before,
            [false, false],
            "OKX-LOAD native unseen queries {index}"
        );
        states.push(before);
        mark_price_loaded(&mut marks, id);
        assert!(marks.contains(id), "OKX-LOAD native mark insertion {index}");
        assert!(
            !indexes.contains(id),
            "OKX-LOAD native mark leaves index absent"
        );
        assert_eq!(marks.len(), index + 1, "OKX-LOAD native mark count");
        let marked = [
            has_mark_price_loaded(&marks, id),
            has_index_price_loaded(&indexes, id),
        ];
        assert_eq!(marked, [true, false], "OKX-LOAD native marked queries");
        states.push(marked);
        mark_price_loaded(&mut marks, id);
        assert_eq!(
            marks.len(),
            index + 1,
            "OKX-LOAD native duplicate mark count"
        );
        let duplicate = [
            has_mark_price_loaded(&marks, id),
            has_index_price_loaded(&indexes, id),
        ];
        assert_eq!(
            duplicate,
            [true, false],
            "OKX-LOAD native duplicate queries"
        );
        states.push(duplicate);
        index_price_loaded(&mut indexes, id);
        assert!(
            indexes.contains(id),
            "OKX-LOAD native index insertion {index}"
        );
        assert_eq!(indexes.len(), index + 1, "OKX-LOAD native index count");
        let loaded = [
            has_mark_price_loaded(&marks, id),
            has_index_price_loaded(&indexes, id),
        ];
        assert_eq!(loaded, [true, true], "OKX-LOAD native fully loaded queries");
        states.push(loaded);
        native.push(states);
    }
    assert_eq!(marks.len(), 5, "five independent mark entries");
    assert_eq!(indexes.len(), 5, "five independent index entries");
    assert!(
        marks.contains(&vec![0xd800]) && indexes.contains(&vec![0xdc00]),
        "OKX-LOAD native exact lone units"
    );
    assert!(
        !marks.contains(&vec![0xfffd]),
        "OKX-LOAD native no surrogate substitution"
    );
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/price-load-oracle.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&ids).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Vec<Vec<[bool; 2]>> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(source.len(), 5, "source string profile floor");
    for rows in &source {
        assert_eq!(rows.len(), 4, "source operation floor");
    }
    assert_eq!(native, source, "live membership after native UTF16 state");
}
