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

## Independent plan refinements

Caller/adversarial review distinguishes error timelines: decoded short/malformed
candle rows reject with exactly "invalid KuCoin Spot candles" and no correlated
error companion, just like unsupported preflight. Envelope, transport and stream
failures retain the companion before rejection. Tests must pin both timelines;
do not route every failure through one companion-emitting adapter.

Native compatibility controls explicitly retain duplicate timestamps, oversized
responses, volume column5 despite extra columns, and HTTP for unknown symbols.
No new range validation or local limit is permitted. Add separate genuine
initial registry-capacity and non-finite-clock refusal controls: initial Read
rejection has no outcome and no companion. Add immediate managed-queue refusal
with NotSent, its existing error companion and fixed-clock recovery. Fill genuine
background roots/queued reads in an earlier no-ID callback, never inside the
command entry being tested; explicitly retire them before recovery.

All four fingerprints and the full map were independently verified CLEAN. The
shared Account failure hash remains at count2. HMAC owner confirmed the committed
plan does not overlap its private regions. No production edit or native receipt
is claimed by this plan.

## Native tests-first checkpoint

Original tests-first068e6dff003 exposed11 assertion/missing-authority RED controls
and three native compatibility GREEN controls. During the first managed run, the
two saturation probes exposed a fixture issue: Disabled public feeds suppress
SetTickerSymbols before it reaches the wrapper. The corrected Capacity-only
fixture uses Direct feeds, settles the native bullet first, and asserts the
actual256/one held roots and zero/256 parked preparations. It explicitly retires
them and asserts empty ownership before same-clock recovery. No assertion was
weakened.

The corrected tests were re-executed on the exact original source snapshot and
again show11 RED/three GREEN; the production bytes were backed up and restored
byte-identically. Current managed source passes all14 controls, including
previously unexecuted recovery assertions. Independent corrected-test and
source/adversarial reviews are CLEAN (source SHA256d629d9898a1e, test58bb16996ea0).
Caller source review is CLEAN, architecture No. Full native/strict gates and
current-landing integration remain separate checks.

## Production and focused gates

Corrected tests-firstd589f5a44bd is separate from production. The managed source
passes all14 controls and all575 KuCoin/Binance native tests across42 targets.
Strict scoped all-target/all-feature Clippy and scoped formatting pass. Caller,
source/adversarial and full-map implementation reviews are CLEAN; architecture No.
QUALITY names queued physical requests under one background root explicitly,
without claiming256 logical leg Contexts.

The exact own prune removes only the four planned hashes:84->80 occurrences,
74->70 hash rows, nine files, Spot mod20->16. Global checker/bans still require
incoming BingX/Lighter/MEXC manifest prunes via genuine current-landing integration.
The13 ratchet regressions already pass. Final CI follows that integration; no
merge readiness or zero-legacy claim is implied by these local gates.

## Actual venue-phase integration proof

Productionbf6f3c3387f genuinely integrates landing680eab0e026 at1a03cff60fd.
Only the four shared docs conflicted: full incoming text is kept first, followed
by our appendix/index row once verbatim. Independent preservation review is
CLEAN: exactly eight intended paths differ, all incoming unowned files/prunes
are preserved, and KuCoin/Binance/engine/crypto/root Cargo blobs exactly match
the locally tested production. Incoming signer/registry/host feature declarations
are preserved and will receive fresh full CI; no source-equivalent CI shortcut
is claimed for this new PR.

Integrated14 native controls, strict scoped Clippy/fmt, bans, current request-context
checker and all14 current checker regressions PASS. The full575 native/42-target
run covers the identical runtime/dependency blobs. The previous13-check count
records the earlier checker snapshot; incoming checker now contains14 tests.
Source/adversarial, caller/documentation, full-map and integration reviews are
CLEAN; architecture No. The QUALITY queued-request wording was corrected per
caller review. CI and merge-train authorization remain required before merge.
