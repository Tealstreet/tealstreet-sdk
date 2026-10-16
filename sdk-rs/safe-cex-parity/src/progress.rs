use crate::Result;

pub(crate) const PHASES: [&str; 10] = [
    "P0 foundations",
    "P1 pure core",
    "P2 transport and signing",
    "P3 public data",
    "P4 tier-1 HMAC venues",
    "P5 remaining venues",
    "P6 exchange consumers",
    "P7 native hosts",
    "P8 browser",
    "P9 retirement",
];

pub(crate) fn phase(source: &str) -> &'static str {
    if source.contains("/candleCache/") || source.contains("/utils/ohlcv/") {
        return PHASES[3];
    }
    if source.starts_with("packages/http-over-websocket/src/") {
        return PHASES[2];
    }
    if source.starts_with("packages/script-api-core/src/order/") {
        return PHASES[1];
    }
    let Some(path) = source.strip_prefix("packages/safe-cex/src/") else {
        return "unassigned";
    };
    if path == "health/errorPatterns.ts" {
        return PHASES[4];
    }
    if path == "utils/egressRegion.ts" {
        return PHASES[2];
    }
    if ["webworker/", "journal/", "accounts/", "crossTrade/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
    {
        return PHASES[6];
    }
    if let Some(exchange) = path.strip_prefix("exchanges/") {
        if (exchange.contains("ws") && exchange.contains("public"))
            || exchange.contains("public-data")
        {
            return PHASES[3];
        }
        if let Some((family, _)) = exchange.split_once('/') {
            return if ["bybit", "binance", "okx", "bitget", "blofin", "bitunix"].contains(&family) {
                PHASES[4]
            } else {
                PHASES[5]
            };
        }
        return PHASES[4];
    }
    if ["transport/", "httpOverWebsocket/", "config/"]
        .iter()
        .any(|prefix| path.starts_with(prefix))
        || path == "safeCexAxiosCreate.ts"
        || [
            "payloadSigner",
            "signWsAuth",
            "retryReadsOnly",
            "remoteSign",
            "RemoteSign",
            "tickIntervalOverride",
            "wsAuth",
            "Websocket",
        ]
        .iter()
        .any(|name| path.contains(name))
    {
        return PHASES[2];
    }
    PHASES[1]
}

// Consume report's validated counters; no symbol/status counting happens here.
pub fn from_report(report: &str) -> Result<String> {
    // Pin report's status schema before interpreting its positional counters.
    const HEADER: &str = "scope\tverified ported rows / total rows\tdistinct validated Rust items claimed\tdivergent\tnot-applicable";
    if report.lines().nth(1) != Some(HEADER) {
        return Err("unsupported report column schema".into());
    }
    let mut overall = None;
    let mut phases = Vec::new();
    let mut pending = Vec::new();
    for line in report.lines().skip(2) {
        let fields: Vec<_> = line.split('\t').collect();
        if fields.len() != 5 {
            return Err(format!("invalid report row: {line}"));
        }
        let fraction = fields[1];
        if fields[0] == "total" {
            overall = Some(fraction);
        }
        if let Some(phase) = fields[0].strip_prefix("phase ") {
            phases.push(format!("{phase}\t{fraction}"));
        }
        if let Some(file) = fields[0].strip_prefix("file ") {
            let (ported, total) = fraction
                .split_once(" / ")
                .ok_or("invalid report fraction")?;
            let number = |s: &str| {
                s.parse::<usize>()
                    .map_err(|_| format!("invalid report number: {s}"))
            };
            let pending_rows = number(total)?
                .checked_sub(number(ported)? + number(fields[3])? + number(fields[4])?)
                .ok_or("invalid report status totals")?;
            if pending_rows > 0 {
                pending.push((pending_rows, file));
            }
        }
    }
    let mut output = format!(
        "{}\nOverall\t{}\nPhase source-file allocation (not phase completion):\n",
        report.lines().next().ok_or("empty report")?,
        overall.ok_or("report has no total row")?
    );
    for phase in phases {
        output.push_str(&format!("{phase}\n"));
    }
    pending.sort_by(|(a, left), (b, right)| b.cmp(a).then(left.cmp(right)));
    output.push_str("Largest pending source files:\n");
    for (count, file) in pending.into_iter().take(10) {
        output.push_str(&format!("{count}\t{file}\n"));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn phases_follow_plan_source_ownership_and_new_backend_roots() {
        for (path, expected) in [
            ("packages/safe-cex/src/types.ts", PHASES[1]),
            ("packages/safe-cex/src/health/errorPatterns.ts", PHASES[4]),
            ("packages/safe-cex/src/utils/egressRegion.ts", PHASES[2]),
            (
                "packages/safe-cex/src/utils/nativePositionSize.ts",
                PHASES[1],
            ),
            ("packages/safe-cex/src/transport/direct.ts", PHASES[2]),
            (
                "packages/safe-cex/src/exchanges/bitget/bitget.ws-secondary-public.ts",
                PHASES[3],
            ),
            (
                "packages/http-over-websocket/src/RelayWebSocket.ts",
                PHASES[2],
            ),
            (
                "packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts",
                PHASES[3],
            ),
            (
                "packages/safe-cex-server/src/candleCache/backend.ts",
                PHASES[3],
            ),
            (
                "packages/safe-cex/src/exchanges/bybit/bybit.exchange.ts",
                PHASES[4],
            ),
            (
                "packages/safe-cex/src/exchanges/hyperliquid/hyperliquid.exchange.ts",
                PHASES[5],
            ),
            (
                "packages/safe-cex/src/webworker/ExchangeWorker.ts",
                PHASES[6],
            ),
            ("packages/new-root/src/thing.ts", "unassigned"),
        ] {
            assert_eq!(phase(path), expected, "{path}");
        }
    }
}

#[cfg(test)]
mod schema_tests {
    use super::from_report;

    #[test]
    fn progress_rejects_reordered_or_extended_report_columns() {
        let header = "scope\tverified ported rows / total rows\tdistinct validated Rust items claimed\tdivergent\tnot-applicable";
        let report = format!("Basis: validated\n{header}\ntotal\t1 / 5\t3\t1\t1\n");
        assert!(from_report(&report).unwrap().contains("Overall\t1 / 5"));
        for changed in [
            report.replace("divergent\tnot-applicable", "not-applicable\tdivergent"),
            report.replace("not-applicable\n", "not-applicable\tdeferred\n"),
            report.replace("distinct validated Rust items claimed", "pending"),
        ] {
            assert_eq!(
                from_report(&changed).unwrap_err(),
                "unsupported report column schema"
            );
        }
    }
}
