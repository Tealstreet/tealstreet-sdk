# Health, HTTP, token and error-reporting evidence

The final package-local integration gate passes 13 tests: three in `core_error_reporting` and ten in `core_health_http_token`. The classifier test evaluates 686 independently generated TypeScript cases (including all four JavaScript line terminators). The emitter test evaluates 18 TypeScript event/report scenarios across main-window and worker paths. The shared test Store implements `safe_cex_store::Store` directly; it introduces no production Store projection.

Commands run successfully from the repository root:

```sh
cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange --all-features --test core_error_reporting --test core_health_http_token
cargo clippy --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-exchange --all-features --test core_error_reporting --test core_health_http_token
node sdk-rs/safe-cex-exchange/scripts/generate-health.cjs --check
node sdk-rs/safe-cex-exchange/scripts/generate-error-reporting.cjs --check
```

Clippy uses the workspace deny table. Health is optional on the exchange crate; `--all-features` is necessary to exercise its compiled regexes and classifier corpus. Formatting of the owned modules and integration fixtures was checked with `rustfmt --edition 2024 --check`.

Every mutation below was restored. Test controls failed their intended assertion, rather than failing compilation; the restored suites passed again. The first five controls ran before the crate moved from `safe-cex` to `safe-cex-exchange`, with the same named assertions and behavioral code.

| Controlled break | Assertion that failed |
| --- | --- |
| Replace a merged category's first severity with the later severity | `health_first_severity_and_suppression` |
| Restart recovery on every warning | `health_order_severity_timer_and_latched_counts` |
| Remove timestamps at the exact 60-second cutoff | `stats_inclusive_boundary_and_status_helpers` |
| Skip proxy authorization | `http_request_order_capture_and_error_count_asymmetry` |
| Disable joining an existing token fetch | `token_singleflight_worker_fallback_refresh_and_silent_set` |
| Enable Rust Unicode case folding on the generated JS regexes | `health_generated_typescript_oracle` |
| Retain the outer originalMessage after an enhanced formatter spread supplied undefined | `base_error_events_match_live_typescript_oracle` |
| Append an HTTP reason phrase alongside the status | `http_failure_reads_status_only_and_correlates_once_boundary` |
| Clone the nearby HTTP failure instead of consuming it | `base_error_events_match_live_typescript_oracle` failed the second-warning scenario |
| Freeze the successful-fetch token passed to all callbacks | `token_fetch_callbacks_read_live_cache_but_broadcast_callbacks_use_payload` |
| Corrupt a generated classifier category | Health generator rejected pattern drift |
| Corrupt the generated error-reporting message | Error-reporting generator rejected oracle drift |

The TypeScript oracle exposed the enhanced-error undefined-spread discrepancy before its correction. Only `originalMessage` overlaps the formatter's optional properties with the outer telemetry fields; removing that overwritten value reproduces serialization, while legacy and explicit Sentry extra values still take their declared priority.

The emitter oracle also verifies correlation is consumed once, successful responses clear it, stale records disappear, and enhanced causal errors preserve it for a subsequent unenhanced error. The token tests verify dropped initiating callers do not cancel shared fetch progress, workers fall back immediately to direct fetch, direct setters are silent, and fetch callbacks reread the live cache while broadcast callbacks receive the broadcast payload.

This file records the bounded modules' gates. Workspace-wide parity, wasm, default-feature and final adversarial review outcomes belong in the coordinator's gate evidence.

`proxyTokenExpiry.ts::isTokenExpired` is mapped to the public shared `proxy_token::is_token_expired(token, &VirtualClock)`. ProxyTokenClient delegates to it; the future accessToken owner can reuse it without a second clock or decoder. The async-context provider is outside this chunk. The live expiry oracle covers 486 token/clock pairs; a raw-clock mutation fails the shared server-offset assertion. The live Axios oracle covers 64 cases; disabling timeout or network classification in the actual TypeScript oracle produces the expected mismatch. Restored gates pass.

The health dot translator is pinned by CR/U+2028/U+2029 cases. Reverting its explicit line-terminator class makes the live health comparison fail on a message the TypeScript does not classify.

After restoring the correlation mutation and the coordinator's independent controls, `core_contract` also passed all four tests. The test Store's `markets_by_symbol` getter now builds a shared-contract map from actual Market rows, with the last duplicate replacing earlier values. A combined run of the three integration suites passed 16 tests, and strict clippy passed for those three targets.
