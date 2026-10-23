# KuCoin Spot managed candle command

Fresh branch feat/rust-kucoin-spot-candle-context starts at actual1854 plus
landing8d57199e884. Use only landed command-entry, read-delivery, typed expiry
and static-header APIs. No unlanded history bridge, signer, Plan or group API.
Open PR file lists currently show no KuCoin source/test claims. HMAC owner
claude-4yjlw0 explicitly has no objection; private-family work waits on1817.
Recheck before source/tests; keep sign.rs and private request/credentials intact.

The full inventory and remaining prerequisites are committed in
[RUST_KUCOIN_REQUEST_CONTEXT.md](RUST_KUCOIN_REQUEST_CONTEXT.md). Current scanner
manifest is84 occurrences/74 hash rows/nine files (futures52, Spot32). This
slice affects only spot/mod.rs (20 occurrences), its native tests and shared
docs/own inventory prune. Four exact count-one fingerprints are candidates:

| Fingerprint | Current expression | Shared destination |
| --- | --- | --- |
| 4d14eae20fff4dab766584ec10ba890cc51f49fbf86cf1e5d95860110212dbf1 | public streamed helper | request_managed_with_headers plus stream_response; delete helper after proving candles are its sole caller. |
| ae51780dba21824a7515bfffef90bd4f78599ea6af3cc40ef36db98ec6dc93a4 | invalid candle numeric failure | fail_context on original Read root. |
| 9eb2532ffa305903307527ed26561752ae2716a465c7347f6bb03d38987efb3d | candle numeric success | done_context on original Read root. |
| 08746e27d5c5b4fa82d1534a206c0f5932b85da2bc45429c8e9bd888544ebff4 | unsupported candle interval | typed fail_context after genuine entry admission. |

All other80 occurrences/70 rows retain the full inventory's destinations. The
shared Account numeric failure expression remains after Candles leaves that
match arm; do not prune it. Spot signed request, account/history/trading/bullet
and all futures paths remain unchanged. Preserve Spot successful no-op and
accountMarginType capability policy. Expected84->80 is scanner-dependent, never
an exemption or a zero claim.

At the genuine FetchCandles callback, admit a Public Read root with the existing
20-second/Never policy before interval preflight or I/O; either entry error
returns without numeric fallback. Replace Pending::Candles.command with Context.
Issue unsigned GET through actual static ordered sign::PUBLIC_HEADERS, then
stream the existing data envelope. Validate exact read HTTP delivery with the
original root and exact single physical leg before row decode/staging and before
response validation/publication. Keep existing discovery background retirement
separate; command roots must not be passed through discovery finish_http_context.

Retain typed VenueError through candle preparation and HTTP failure callbacks;
other native/private callbacks stay unchanged. Use shared command_error_context
immediately before fail_context when preserving the existing correlated native
error companion; no timed validator may suppress expiry cleanup. Initial
unsupported preflight retains its no-companion behavior, with Unsupported/NotSent
kind. Native envelope/row errors retain their text. No local status ladder,
retry, completion helper or authority policy. Architecture answer No.

Preserve exact URL/query order: symbol, type, startAt, endAt; existing flooring,
u64 casts, interval mapping, no-limit500, cap1500 and from_ms semantics remain.
Preserve native candle codec, minimum six columns, seconds/milliseconds timestamp
normalization, quantities and timestamp sorting; no new row filtering or local
limit truncation. Header bytes/order and empty body remain keyed/keyless equal.

Commit plan, obtain independent source/adversarial/caller/full-map review, then
separate tests-first assertion RED on actual native source. Controls: native
unsigned wire/header/limit/range/codec compatibility; Prepare/Bind/Claim; rejection
of unreleased, unclaimed, stale and expired replies; typed parked preparation,
released transport and physical expiry cleanup; atomic malformed/native-failed
page; duplicate entry and same-ID late response; fixed-clock270 repeated
preflight and success/failure retirement. Preserve original native fixture
payloads/assertions while adopting existing prepared transport plumbing.

Implement, prune only actual four own removals, append landed caller/audit/quality
behavior and re-review. Execute KuCoin+Binance all-feature native tests, scoped
strict Clippy/fmt, bans and both request-context checks. CI owns WASM/replay gates.
Only the overseer's merge train/window plus fresh separate guard may merge; use
current landing, keep all incoming doc text/prunes, and record executed evidence.
