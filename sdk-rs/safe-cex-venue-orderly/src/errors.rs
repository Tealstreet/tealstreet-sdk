//! Orderly's expected order-rejection classification and logging hints.
use regex::{Regex, RegexBuilder};
use std::{collections::BTreeMap, sync::LazyLock};

static EXPECTED_ORDERLY_ORDER_REJECTION_PATTERNS: LazyLock<[Regex; 3]> = LazyLock::new(|| {
    [
        r"The order value should be greater or equal to [\d.]+",
        r"The margin will be insufficient after",
        r"The TP_SL side should be opposite to position side",
    ]
    .map(|pattern| {
        RegexBuilder::new(pattern)
            .case_insensitive(true)
            .unicode(false)
            .build()
            .expect("literal Orderly rejection regex")
    })
});

pub fn is_expected_orderly_order_rejection(message: Option<&str>) -> bool {
    message.is_some_and(|message| {
        EXPECTED_ORDERLY_ORDER_REJECTION_PATTERNS
            .iter()
            .any(|pattern| pattern.is_match(message))
    })
}

pub fn get_orderly_order_error_log_options(
    message: Option<&str>,
) -> Option<BTreeMap<String, bool>> {
    is_expected_orderly_order_rejection(message)
        .then(|| BTreeMap::from([("suppressConsoleLog".into(), true)]))
}
