mod support;
use safe_cex_engine::checksum::Crc32;
use support::*;

// Fixed wire CRCs were calculated independently with Python zlib.crc32.
#[test]
fn shared_crc_preserves_native_decimal_spelling_and_recovery() {
    let mut crc = Crc32::new();
    crc.update(b"99.00:1.000");
    crc.update_byte(b':');
    crc.update(b"101.00:3.000:98.0:2.00");
    assert_eq!(crc.finish(), 2_709_419_053);

    let mut h = Harness::public();
    h.book();
    let snapshot = r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"snapshot","data":[{"seq":10,"bids":[["98.0","2.00"],["99.00","1.000"]],"asks":[["101.00","3.000"]],"checksum":-1585548243}]}"#;
    h.message(snapshot);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(99.0));
    // An unchecked update still maintains the exact text used by the next CRC.
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":10,"seq":11,"bids":[["99.00","1.500"]]}]}"#);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"pseq":11,"seq":12,"checksum":1828144228}]}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids()[0], (99.0, 1.5));
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"bids":[["99.00","0"],["98.0","2.500"],["97.000","4.0"]],"checksum":1443586883}]}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids(), [(98.0, 2.5), (97.0, 4.0)]);
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"bids":[["98.0","5"]],"checksum":1}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"bids":[["99.00","1.000"]],"checksum":-1585548243}]}"#);
    assert!(h.engine.book("BTCUSDT").is_none());
    h.message(snapshot);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(99.0));
}

#[test]
fn shared_crc_keeps_top_25_interleaving_with_an_unequal_side() {
    assert_eq!(Crc32::default().finish(), 0);
    let mut h = Harness::public();
    h.book();
    let bids = (75..=100)
        .map(|price| format!(r#"["{price}","1"]"#))
        .collect::<Vec<_>>()
        .join(",");
    h.message(&format!(r#"{{"arg":{{"channel":"books","instId":"BTCUSDT"}},"action":"snapshot","data":[{{"bids":[{bids}],"asks":[["101","2"]],"checksum":-1334613043}}]}}"#));
    assert_eq!(h.engine.book("BTCUSDT").unwrap().bids().len(), 26);
    // The previously excluded 26th row enters the checksum after deletion.
    h.message(r#"{"arg":{"channel":"books","instId":"BTCUSDT"},"action":"update","data":[{"bids":[["100","0"]],"checksum":276382855}]}"#);
    assert_eq!(h.engine.book("BTCUSDT").unwrap().best_bid(), Some(99.0));
}
