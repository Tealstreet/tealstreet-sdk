# Asterdex account reads and configuration — eight rows

Started directly at integration `c2e36c3d08a9b98d0431bd2e306fbe60c3aab82d` on `feat/rust-asterdex-account-fetch-8`. No unreviewed external lane ancestor. Code gates below were measured at **`b0868874c8275d9ad91870364fd1135f43eb8f23`**, before the evidence commit; they are not measurements of a later merge/doc SHA.

`tomllib` census: **1 pending→ported, 7 pending→divergent**. Before: 123 ported /66 divergent /42 pending. After: 124 /73 /34. Iteration 1 stopped after eight manifest files/231 rows and eight independently named changes. The seven divergent rows have seven distinct FORCED notes and actual method targets. This is a census, not proof of all input domains.

The coherent unit implements reconciliation, mode synchronization, positions/orders/markets/tickers reads, leverage and mode changes on real BaseExchange/Store/configured REST contracts. Full exchange construction, startup and API construction remain pending. The caller polls Rust futures. Retained-token query decoding is untouched; no QueryValue conversion shim/projection was added.

| Gate at code SHA | Exit |
|---|---:|
| `cargo check -p safe-cex-venue-asterdex --all-targets --jobs 2` | 0 |
| `cargo test -p safe-cex-venue-asterdex --all-targets --jobs 2 -- --include-ignored --test-threads=2` | 0 |
| `parity:extract:check` (output: zero updates/additions/orphans) | 0 |
| `scripts/account-fetch-census.py` | 0 |

Cargo used `--manifest-path sdk-rs/Cargo.toml`; oracle compiler path was `/Users/sam/cs/tealstreet-next/node_modules/typescript`. Exact commands/output/exits live in ACCOUNT_FETCH_EVIDENCE.json.

Native assertions compare decoded positions, orders, markets, tickers, balance, loaded flags, settings, timestamps, notifications and logs **before any request comparison**, across all scenarios. Info fields are checked recursively with binary64 numeric bits, then remaining complete model fields are compared; no re-encoding. Request observations remain untransformed. Ordinary model float equality is not an exhaustive signed-zero proof; leverage has an explicit `-0` bit witness, including a `+0` maximum clamp. Source uses unedited members and real DefaultStore, helpers and BaseExchange.dispose; lifecycle seams contain no sockets/tasks. Iteration 1 stopped at **30 named scenarios /36 independently expected requests /32 maximum fixture polls**.

| Compiled mutation family | Named failing assertion | Mutation / restored exits |
|---|---|---|
| Reads: discard partial bracket map | `markets-bracket-null-tail-keeps-partial native markets` | 101 /0 |
| Reconciliation: invert version guard | `balance-current-applied native applied` | 101 /0 |
| Mode: invert synchronized Store setting | `sync-hedge-string native hedged` | 101 /0 |
| Leverage: change half-up tie to strict-greater | `leverage-in-range-half native rounded leverage` | 101 /0 |

Iteration 1 stopped after these **four** compiled controls. Earlier f714 code gates and two completed controls are retained in ACCOUNT_FETCH_SUPERSEDED.json; its remaining mode control was interrupted (runner130), exact source restored, and no unfinished verdict is claimed. Those records do not cover final code. Zero survivors; exact production bytes restored. Full patches, original hashes and outputs live in ACCOUNT_FETCH_CONTROLS.json. The runner pins clean HEAD, unique mutation sites and SIGTERM restoration; compilation failures do not count as catches.

Read-only sub-agent review resolved null-body request ordering and demoted the draft sync ported claim after a real AxiosError/native-name witness, plus partial bracket loss, null-row silent filtering, clamping-masked rounding tests, source disposal fixture mismatch, signed-zero clamp behavior and mutation restoration/site checks. The adjacent existing `parse_leverage_brackets_response` now preserves partial entries after a later failure; it is **not counted as a newly moved row**. Earlier development compile/assertion failures were corrected before the named code gates; those uncommitted attempts are not claimed as final-SHA evidence.

Seven FORCED boundaries are explicit in their own notes: sync AxiosError-name projection; strict position records; closed order enums/owned info; strict trading-market/bracket values; strict ticker records/owned info; non-finite untransformed leverage refusal; and mode-refresh inheritance of strict position ingress. No complete malformed-value/prototype/surrogate/alias/Promise-scheduler census is claimed.

Not run: workspace builds/tests, other venues, host clippy, wasm checks/clippy/tests, doctests, CI, real HTTP/signing/browser/wallet integration, full exchange startup/constructor/socket assembly, or exhaustive payload enumeration. No shared type or trait changed.

Merge-down/preservation evidence is appended after integration synchronization; original Cargo exits stay attached to their measured SHA.

## Integration merge-down

Merged integration `f6c865ddd8331c32516f063d90d045761bd2c73b` at merge SHA `f6dd84f03bfae42d91e60cc7ebe859c4f635d63e`. At that merge SHA, `git diff --exit-code b0868874c8275d9ad91870364fd1135f43eb8f23 HEAD -- sdk-rs/safe-cex-venue-asterdex/src sdk-rs/safe-cex-venue-asterdex/tests` exited **0**. Selected dependency/oracle source inputs (including shared TradeTunnel frames) and the reachable Asterdex Cargo.lock graph also compare unchanged (0). Unreachable Bitget/Blofin lock records changed; package.json changed only the Kucoin oracle command, while the three independently selected Asterdex/extractor scripts compare unchanged (0).

Post-merge extractor and independent census both exited **0** at that merge SHA. Original Cargo gates and all four compiled controls remain measurements of code SHA `b0868874c8275d9ad91870364fd1135f43eb8f23`; Cargo was not rerun at the merge SHA because the selected inputs are preserved. Exact scopes/commands/output/lock graph comparison script are committed in ACCOUNT_FETCH_MERGE_EVIDENCE.json. This final evidence update changes documentation/records only.
