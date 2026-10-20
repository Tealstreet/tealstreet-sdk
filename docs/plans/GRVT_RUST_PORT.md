# GRVT perpetuals Rust port

Branch `feat/rust-grvt`, based on `origin/feat/rust-rewrite`. PR and merge target is `feat/rust-rewrite`. Full auto; no stash, rebase, force push or master changes.

## Phases and verification

1. Public data: typed streamed instrument catalog and signing metadata; TS-compatible ticker seed and delta subscriptions (20 per 100 ms); complete 50-level book snapshots, sequence recovery; live and historical candles. Production/testnet real URLs, public channel, liveness/reconnect/resubscribe. Scripted public tests, fmt/clippy/bans, independent phase review and fix/commit.
2. Private data: session state machine, authenticated REST snapshots, per-market leverage limits/settings, private order/position/fill streams, reconciliation guards. Main-account builder authorization, sub-account discovery. Failed/malformed snapshots never clear good state. Header-capable private channel routed by landed hosts. Scripted auth/account/reconciliation tests and independent phase review.
3. Trading: exact EIP-712 order signing (detailed below), canonical nonce/client IDs, preflight entire command before writes, sequential creates/cancels, protective orders, safe cancel/recreate amend, leverage/mode capabilities. Never resend ambiguous writes. Independent signing oracle and scripted command tests, phase review.
4. Queries: typed paginated fills/order history/ledger/public trades and client-ID lookup where documented; correct JSON command shapes. Cursor completeness, time and exact-ID boundaries. Independent phase review.
5. Harness gates: register crate/WASM/SDK/harness/CI; documented private tape with independently audited authentication/signing, real public 20-second two-book capture and committed trimmed fixture. All brief gates, Binance regressions, package checks; independent phase review. Record CPU/memory/size and test counts.
6. Landing: merge latest landing branch, push and PR to `feat/rust-rewrite`; adversarial subagent reviews posted, fix/repeat, green CI only, merge and fresh follow-up branch. Update roster and final gate proof.

## EIP-712 signing plan

- Trace actual `grvt.exchange.ts` and vendored GRVT SDK type templates; the unused signing helper has different chain IDs and is not authoritative. Production domain name `GRVT Exchange`, version `0`, chain 325; testnet 326; no verifying contract.
- Retain validated instrument hash and base/quote decimals with each catalog snapshot. Never synthesize an asset ID or reuse stale metadata after a refresh.
- Human-readable API size/price strings and signed integer size/price must derive from the same adjusted decimal values. Convert base size by instrument base decimals and price by fixed 1e9 using checked decimal arithmetic; validate uint widths, subaccount uint64, nonce uint32 and nanosecond expiry uint64. No float rounding may silently change signed economics.
- Match exact field order/types and dependency hashing for Order/OrderWithBuilderFee and OrderLeg arrays. Use shared Keccak/secp256k1 primitives with typed ABI words, deterministic digest/signature, split r/s/v and signer address. Verify independently against TS/ethers fixed vectors for both chains and builder variants, maxima/invalid decimals and field mutations.
- Builder fee is required by current TS policy: authorized Tealstreet address and cap covering 0.03 percent (signed uint32 300). Unknown/unauthorized/cap-too-low must fail before any create. Sign and transmit matching builder address/fee.
- Caller client IDs must be canonical decimal 1..2^63-1, honored verbatim on one native order. Keep client ID separate from signing nonce; retain bounded nonce/real-ID/creation-time mapping for cancellation and chaser fill queries. Preflight replacements including TIF/reduce-only/trigger metadata before cancelling.

## Session authentication plan

- Login is a POST to real Edge `/auth/api_key/login` with API key and remember-me metadata. Read session from exposed `X-Ts-Grvt-Session` or `Set-Cookie: gravity=...` and account from `X-Grvt-Account-Id`; require both. Engine response headers already cross the host boundary.
- Public MDG reads never receive private credentials. TDG uses session/account headers; landed routing owns proxy/tunnel selection. Private websocket emits real trades `/ws` URL and explicit Private role. Routed session proxy sends the existing auth first frame and waits for `auth_success`; direct native uses Cookie/account upgrade headers. The host supplies routing-derived `session_proxy`; no Tealstreet URL in venue.
- Coalesce refreshes behind one auth request; expire/replace credentials atomically and reject malformed sessions. One explicit code-1000 rejection can refresh auth and retry the original operation once with identical order identity/body. Never retry writes on network/timeout uncertainty. Named POST reads may retry bounded network failures, clearing staged streamed rows first.
- Discover subaccount from aggregated account summary only if not configured; no account-scoped request before discovery. Startup then leverage limits, builder check, positions/open orders and private subscriptions. Balance polling 60 seconds; order and position events schedule debounced reconciliation; generation and last-update guards protect against stale REST responses.
- No credential, session, signature or response-body logging. Auth failure leaves private loaded state truthful and good snapshots intact. Socket liveness covers silent connection/open, ping handling, reconnect and expired-session recovery.

## Progress

- Public phase implemented. Independent review verified live subscription ACKs and fixed four findings: advertised nonzero first delta baseline, independent candle-interval feeds, documented empty-string ticker clears, and stale books after resubscribe. Re-review found unsupported Unicode interval panic; fixed with ASCII validation and regression.
- Public verification: 10 native tests PASS; crate clippy and bans PASS; paired TS public delta regression PASS. Package typecheck/lint running. TS repairs preserve buy/sell rolling volumes/open across deltas, derive quote volume/percentage from current last, and handle clears/nanosecond times. `KNOWN_TS_FIXES.grvt` documents these; no output normalization.
- Signing design reviewed against actual SDK and official examples; exact signatures and native integer bounds have 5 isolated tests PASS. Session/private implementation is staged separately for phase 2 integration.

### Private checkpoint

- Session/private phase independent review PASS after fixes for stale REST rows, exact nanosecond watermarks, authoritative empty snapshot tombstones, reused client IDs and actual CROSS/ISOLATED metadata. Private sequence ACKs, duplicate suppression and gap-triggered reconnect/reconciliation are covered. Proxy auth waits for acknowledgement; native direct uses upgrade headers; only explicit code 1000 gets one preserved-body resend.
- Account tests 13 PASS, public tests 10 PASS, all-target clippy PASS. Paired TS public/private regressions 4 PASS. Public package typecheck/lint passed; repeated checks cover option forwarding/private corrections.
- GRVT real-URL host/WASM/SDK registrations integrated, including routing-derived session-proxy selection. Standalone GRVT native check and release WASM build pass. No host routing policy copied into the venue.
- Live capture: 20 seconds, 4,694 websocket frames, 194 perpetual markets/tickers, two books. Fixture committed in a later gates checkpoint. First public gate matches output and wins CPU (TS 341.9 ms / Rust 263.5 ms; steady 212.4 / 177.2), but memory 1.1 / 1.2 MB FAIL. Memory optimization remains required; no gate thresholds changed.
- Trading and query modules drafted in disjoint files; integration and their independent reviews follow this checkpoint.

### Trading checkpoint

- Independent trading/signing review PASS. Fixed caller-ID shape preflight, client-ID lifecycle anchoring for amendments without assigned native IDs, post-write revision barriers, bounded identity retention and local bracket-parent preservation.
- EIP-712 uses fixed ABI words and prehashed domain/struct; ordinary/builder SDK oracle signatures, both chains, every signed-field mutation and full integer bounds verified. Signed decimal metadata is retained; native integer conversions never use float multiplication. Session resend preserves signed bytes and identity. All non-confirmed writes remain single-attempt.
- Entry + both protective legs preplanned before first write; sequential reducing children keep both TP and SL. Partial failure identifies created IDs. Amend requires validated native semantics, cancel acknowledgement, final CANCELLED and unchanged fill/creation lifecycle before replacing only remaining size. Symbol cancellation uses native kind/base/quote filters. FetchLeverage is a real streamed account read with stale-settings protection.
- Verification: 5 signing + 15 trading + 13 account + 11 public tests PASS (44); all-target clippy PASS. Paired TS signing/semantic/private regression run 31 PASS; existing order/metadata regressions included in separate 17-test run PASS; package typecheck/lint passed at preceding checkpoint and signing helper typecheck passed.
- Paired TS fixes: exact signing/nanosecond integers, both reducing bracket legs, native cancel filters, partial-fill remaining amount, market protective execution, explicit split flag and private ping response. Gate notes list deliberate Rust correctness improvements; no output normalization.
- Public allocation optimization consolidates ticker trackers by catalog index, remaps them across catalog changes, and removes hot-path market clones. Signed GRVT artifact: 785,237 bytes. Public three-run median PASS: main CPU TS 97.5 / Rust 85.8 ms, steady CPU 58.4 / 46.7 ms, retained memory 1.3 / 1.1 MB, 4,695 replayed frames, 194 markets, two books. Re-run after final integration.
- Query module and 12 tests remain an uncommitted draft until the next phase wires and reviews them. Private harness construction in parallel, not yet a passed gate.

### Query checkpoint

- Independent query review PASS after exact nanosecond-bound conversion, zero-client-ID fallback and reused-client lifecycle attribution fixes. Filtered order fills walk to explicit terminal cursor; ambiguous ownership, stalled/malformed cursors and 50-page exhaustion fail INCOMPLETE, never partial success. Network retries/session refresh reset only staged current-page rows and retain committed earlier pages.
- Added native fills, closed-order history, funding income, native fill commission/realized-PnL, public trades and definitive client-ID lookup. Funding payment sign is inverted from GRVT paid-positive into income; no derived closed-position PnL is presented as reported. SDK preserves GRVT's chaser fill-verification hook with complete filtered reads from a safe broad start.
- Verification: 18 history tests; 62 total GRVT native tests PASS; all-target clippy PASS. Independent bounded TS review also PASS: conditional replacements keep native execution price separate from changed trigger price, refreshed cookies/session headers agree, connected private socket refreshes with the session, and mapped orders retain explicit client IDs. Targeted TS 39 tests PASS; full root typecheck 44 tasks PASS, root lint running.
- Native margin-mode mutation and hedged position mode remain explicit unsupported capabilities matching current TS surface; private reads preserve actual native CROSS/ISOLATED settings. Transfers, vaults, options/futures and STABLE_PERP are outside this perpetuals port.
- Public fixture/signature-validated private harness and final integration/CI gates next. No gate is waived.

### Harness integration checkpoint

- Public fixture contains the recorded 20-second, 194-market session with 4,694 WS frames and 75 snapshots across two books. Private tape independently validates EIP-712 signatures, exact native identities/economics, session rejection and identical resend, four single-use writes and individual fills. Initial private gate PASS; harness tests 158 PASS before the added public-credential leak assertion.
- Independent phase-5 review found two harness gaps still open: stale REST sessions remain statically accepted after rotation, and socket auth audit needs per-connection generations to forbid same-connection reauth. These are being fixed before the final gate run.
- Full root typecheck (44 tasks) and lint (28 tasks) PASS. Root tests running. Native workspace check found the pre-existing Bitrue WsConnect test pattern missing new fields; fetched landing already fixes it. Integrate landing before final checks; its new max-slippage model field also requires GRVT initializer updates.

### Harness review closure

- Phase-5 independent re-review PASS: 170 normal harness tests plus three independent adversarial probes PASS. Actual login responses alone rotate per-Tape credentials; code 1000 invalidates them. Old sessions are rejected persistently after rotation. Each real socket generation authenticates on its first frame; same-socket reauth fails and a later correct reconnect cannot erase that failure. Optional generic audit hooks preserve Binance and WOO behavior.
- Latest landing integrated at 784c575c9a, preserving nested workspace and all other venue registrations. GRVT initializers accommodate the new max-slippage field. Default combined WASM build, GRVT 62 native tests, venue clippy and all-venue wasm32 clippy PASS. Post-merge root typecheck 44 tasks and lint 28 tasks PASS.
- Root tests encountered a pre-existing HyroTrader import timeout at five seconds (6,874 other safe-cex tests passed). Focused ten-test control PASS with the correctly forwarded 30-second timeout; full safe-cex rerun uses that command-line timeout only, without changing test settings. Final replay metrics still pending.
- A fresh private-state adversarial audit reproduced three correctness gaps being fixed before landing: late old-session rejection must retain refreshed credentials; newer native stream settings must beat stale leverage reads; login network failures need the TS named-read retry budget. No issue is waived.
