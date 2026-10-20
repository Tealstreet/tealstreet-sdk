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

- Trace actual `grvt.exchange.ts` and vendored GRVT SDK type templates; the unused signing helper has different chain IDs and is not authoritative. Production domain chain 325, testnet 326.
- Retain validated instrument hash and base/quote decimals with each catalog snapshot. Never synthesize an asset ID or reuse stale metadata after a refresh.
- Human-readable API size/price strings and signed integer size/price must derive from the same adjusted decimal values. Convert base size by instrument base decimals and price by fixed 1e9 using checked decimal arithmetic; validate uint widths, subaccount uint64, nonce uint32 and nanosecond expiry uint64. No float rounding may silently change signed economics.
- Match exact field order/types and dependency hashing for Order/OrderWithBuilderFee and OrderLeg arrays. Use shared Keccak/secp256k1 primitives with typed ABI words, deterministic digest/signature, split r/s/v and signer address. Verify independently against TS/ethers fixed vectors for both chains and builder variants, maxima/invalid decimals and field mutations.
- Builder fee is required by current TS policy: authorized Tealstreet address and cap covering 0.03 percent (signed uint32 300). Unknown/unauthorized/cap-too-low must fail before any create. Sign and transmit matching builder address/fee.
- Caller client IDs must be canonical decimal 1..2^63-1, honored verbatim on one native order. Keep client ID separate from signing nonce; retain bounded nonce/real-ID/creation-time mapping for cancellation and chaser fill queries. Preflight replacements including TIF/reduce-only/trigger metadata before cancelling.

## Session authentication plan

- Login is a POST to real Edge `/auth/api_key/login` with API key and remember-me metadata. Read session from exposed `X-Ts-Grvt-Session` or `Set-Cookie: gravity=...` and account from `X-Grvt-Account-Id`; require both. Engine response headers already cross the host boundary.
- Public MDG reads never receive private credentials. TDG uses session/account headers; landed routing owns proxy/tunnel selection. Private websocket emits real URL, explicit Private role and upgrade authentication headers; no Tealstreet URL in venue.
- Coalesce refreshes behind one auth request; expire/replace credentials atomically and reject malformed sessions. One explicit code-1000 rejection can refresh auth and retry the original operation once with identical order identity/body. Never retry writes on network/timeout uncertainty. Named POST reads may retry bounded network failures, clearing staged streamed rows first.
- Discover subaccount from aggregated account summary only if not configured; no account-scoped request before discovery. Startup then leverage limits, builder check, positions/open orders and private subscriptions. Balance polling 60 seconds; order and position events schedule debounced reconciliation; generation and last-update guards protect against stale REST responses.
- No credential, session, signature or response-body logging. Auth failure leaves private loaded state truthful and good snapshots intact. Socket liveness covers silent connection/open, ping handling, reconnect and expired-session recovery.

## Progress

- Initial repository/spec/TS inspection complete; signing and session designs under independent sub-agent scrutiny. Public implementation next.
