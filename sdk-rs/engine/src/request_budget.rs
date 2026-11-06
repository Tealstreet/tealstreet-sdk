//! Closed HTTP wait budgets. TypeScript's requestBudget.ts is the parity reference.
//! These labels grant no retry, credential or execution authority.
use crate::Command;
#[cfg(not(target_arch = "wasm32"))]
use crate::Method;

#[repr(u8)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RequestClass {
    Discovery,
    OrderRead,
    OrderWrite,
    History,
    #[default]
    Read,
}

pub const MAX_REQUEST_BUDGET_MS: u32 = 2_147_483_647;

impl RequestClass {
    #[cfg_attr(target_arch = "wasm32", inline(never))]
    pub(crate) fn for_operation(self, operation: crate::request_context::Operation) -> Self {
        if operation == crate::request_context::Operation::Mutation {
            Self::OrderWrite
        } else if self == Self::OrderWrite {
            Self::OrderRead
        } else {
            self
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Discovery => "discovery",
            Self::OrderRead => "orderRead",
            Self::OrderWrite => "orderWrite",
            Self::History => "history",
            Self::Read => "read",
        }
    }
    pub fn budget_ms(self) -> u32 {
        match self {
            Self::Discovery => 60_000,
            Self::History => 30_000,
            _ => 20_000,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn timeout_ms(self, timeout: Option<u32>) -> u32 {
        match timeout {
            Some(ms) if ms > 0 => ms.min(MAX_REQUEST_BUDGET_MS),
            _ => self.budget_ms(),
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn diagnostic(self, budget_ms: u32, elapsed_ms: u128) -> String {
        format!(
            "class={} budget={budget_ms}ms elapsed={elapsed_ms}ms",
            self.as_str()
        )
    }
    pub(crate) fn venue_action(action: &crate::VenueAction) -> Self {
        match action {
            crate::VenueAction::RefreshMarkets => Self::Discovery,
            crate::VenueAction::FetchClosedPositions { .. } => Self::History,
            _ => Self::OrderRead,
        }
    }
    #[cfg_attr(target_arch = "wasm32", inline(never))]
    pub(crate) fn command(command: &Command) -> Self {
        match command {
            Command::Query {
                query: crate::Query::OrderByClientId { .. },
                ..
            }
            // TS candle clients retain an explicit 20 s HTTP timeout.
            | Command::FetchCandles { .. } => Self::OrderRead,
            Command::Venue { action, .. } => Self::venue_action(action),
            Command::Query { .. } => Self::History,
            // The issued operation selects OrderWrite. Keeping dispatch metadata
            // read-shaped also covers read legs of a mutation without reclassifying
            // the entire command or granting any execution authority.
            _ => Self::OrderRead,
        }
    }
    /// Legacy GET classification. Managed posted reads carry their semantic label.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn legacy(method: Method, url: &str) -> Self {
        if method != Method::Get {
            return Self::OrderWrite;
        }
        // URL components only: queries, fragments and hostnames cannot select a class.
        let Some((scheme, rest)) = url.split_once("://") else {
            return Self::Read;
        };
        if !scheme.eq_ignore_ascii_case("http") && !scheme.eq_ignore_ascii_case("https") {
            return Self::Read;
        }
        let Some(at) = rest.find(['/', '?', '#']) else {
            return Self::Read;
        };
        if at == 0 || !rest[at..].starts_with('/') {
            return Self::Read;
        }
        let path = rest[at..]
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .to_ascii_lowercase();
        let has = |names: &[&str]| path.split('/').any(|part| names.contains(&part));
        if has(&[
            "history",
            "historical",
            "candles",
            "kline",
            "klines",
            "fills",
            "trades",
            "income",
            "allorders",
        ]) {
            Self::History
        } else if has(&["order", "orders", "openorders", "orderstatus"]) {
            Self::OrderRead
        } else if has(&[
            "instruments",
            "instruments-info",
            "contracts",
            "exchangeinfo",
            "markets",
            "symbols",
            "products",
        ]) {
            Self::Discovery
        } else {
            Self::Read
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Case {
        method: String,
        url: String,
        request_class: String,
        timeout_ms: Option<u32>,
        budget_ms: u32,
    }
    #[test]
    fn compact_host_abi_has_no_string_payload() {
        assert_eq!(std::mem::size_of::<RequestClass>(), 1);
        for (code, class) in [
            RequestClass::Discovery,
            RequestClass::OrderRead,
            RequestClass::OrderWrite,
            RequestClass::History,
            RequestClass::Read,
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(class as u8, code as u8);
        }
    }
    #[test]
    fn admission_class_abi_keeps_all_master_codes() {
        assert_eq!(crate::HttpReadClass::Normal as u8, 0);
        assert_eq!(crate::HttpReadClass::Critical as u8, 1);
        assert_eq!(crate::HttpReadClass::Bulk as u8, 2);
        #[cfg(feature = "address-nonce")]
        assert_eq!(crate::HttpReadClass::Reconciliation as u8, 3);
    }
    #[test]
    fn diagnostic_metadata_preserves_the_issued_operation() {
        use crate::request_context::Operation;
        for class in [
            RequestClass::Discovery,
            RequestClass::OrderRead,
            RequestClass::OrderWrite,
            RequestClass::History,
            RequestClass::Read,
        ] {
            assert_eq!(
                class.for_operation(Operation::Mutation),
                RequestClass::OrderWrite
            );
            assert_ne!(
                class.for_operation(Operation::Read),
                RequestClass::OrderWrite
            );
        }
    }
    #[test]
    fn ts_reference_class_budgets_and_explicit_bounds_match() {
        let cases: Vec<Case> = serde_json::from_str(include_str!(
            "../../../packages/http-over-websocket/src/requestBudget.fixtures.json"
        ))
        .unwrap();
        for case in cases {
            let method = match case.method.as_str() {
                "GET" => Method::Get,
                "POST" => Method::Post,
                "DELETE" => Method::Delete,
                _ => panic!("unsupported fixture method"),
            };
            let class = RequestClass::legacy(method, &case.url);
            assert_eq!(class.as_str(), case.request_class, "{}", case.url);
            assert_eq!(class.timeout_ms(case.timeout_ms), case.budget_ms);
        }
        for class in [
            RequestClass::Discovery,
            RequestClass::OrderRead,
            RequestClass::OrderWrite,
            RequestClass::History,
            RequestClass::Read,
        ] {
            assert!((1..=MAX_REQUEST_BUDGET_MS).contains(&class.budget_ms()));
            assert_eq!(class.timeout_ms(Some(0)), class.budget_ms());
            assert_eq!(class.timeout_ms(Some(u32::MAX)), MAX_REQUEST_BUDGET_MS);
        }
        assert_eq!(
            RequestClass::History.diagnostic(30_000, 30_012),
            "class=history budget=30000ms elapsed=30012ms"
        );
    }
}
