use safe_cex_venue_hyperliquid::order_response::{
    HyperliquidOrderResponseError, HyperliquidUnreadableOrderResponseError,
    parse_hyperliquid_order_response,
};
#[test]
fn unreadable_constructor_owns_exact_name_and_arbitrary_message() {
    let mut completed = 0;
    for message in ["", "arbitrary diagnostic", "雪😀", "embedded\0text"] {
        let error = HyperliquidUnreadableOrderResponseError::new(message);
        assert_eq!(
            error.name, "HyperliquidUnreadableOrderResponseError",
            "ERROR constructor native name"
        );
        assert_eq!(
            error.message, message,
            "ERROR constructor native original message"
        );
        assert_eq!(
            error.to_string(),
            if message.is_empty() {
                "HyperliquidUnreadableOrderResponseError".to_owned()
            } else {
                format!("HyperliquidUnreadableOrderResponseError: {message}")
            },
            "ERROR native Display"
        );
        completed += 1;
    }
    assert_eq!(completed, 4, "ERROR independent constructor floor");
    let HyperliquidOrderResponseError::Unreadable(error) =
        parse_hyperliquid_order_response(&serde_json::Value::Null).unwrap_err()
    else {
        panic!("ERROR expected unreadable")
    };
    assert_eq!(
        error.name, "HyperliquidUnreadableOrderResponseError",
        "ERROR parser uses actual constructor"
    );
}

#[test]
fn actual_maintained_constructors_follow_native_assertions() {
    unreadable_constructor_owns_exact_name_and_arbitrary_message();
    let result = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/finding-constructors-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "CONSTRUCTORS actual source {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let marker: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("CONSTRUCTORS source completion marker");
    assert_eq!(
        marker["count"], 10,
        "CONSTRUCTORS independent source completion floor"
    );
    assert_eq!(marker["last"], 9, "CONSTRUCTORS source stop9");
}
