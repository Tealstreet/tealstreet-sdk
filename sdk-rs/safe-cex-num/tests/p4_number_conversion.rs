#[test]
fn number_from_string_is_callable_by_venue_decoders() {
    for (text, value) in [
        ("", 0.0),
        (" \u{feff}", 0.0),
        ("0x10", 16.0),
        ("0b10", 2.0),
        ("0o10", 8.0),
        (" 1.25 ", 1.25),
        ("Infinity", f64::INFINITY),
        ("9007199254740993", 9007199254740992.0),
        ("-0", -0.0),
    ] {
        assert_eq!(
            safe_cex_num::number_from_string(text).to_bits(),
            value.to_bits(),
            "{text}"
        );
    }
    for text in ["12abc", "0x", "-0x10", "inf", "\u{0085}"] {
        assert!(safe_cex_num::number_from_string(text).is_nan(), "{text}");
    }
}
