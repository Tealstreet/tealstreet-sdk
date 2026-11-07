# Kraken public discovery and local command custody

Plan first, 2026-10-24. Owner codex-hcmq3f; branch
feat/rust-kraken-public-local-context on actual ba3d7da1d3a (including landed
read-root expiry #1976). This follows Polymarket draft #1995; only this plan is
prepared while that draft is in CI. No source or inventory adoption is claimed.

## Inventory and exact boundary

At the accepted lane count, Kraken has 41 fingerprints / 47 occurrences. The
selected group contains 18 fingerprints / 20 occurrences. One is the successful
Futures SetPositionMode(false) no-op: it remains legacy, inventoried and held on
#1989's genuine keyless done_local_setting seam. Thus this slice targets 17
fingerprints / 19 occurrences, leaving 24 / 28. Do not cast the setter to a
Read, supply a fake credential, or manufacture an accepted write attempt.

Physical public scope is four independent, one-response reads behind three
lexical sends: Futures instruments and tickers (streamed), and Spot AssetPairs
and Ticker (the shared unsigned GET sender). Each discovery/reseed attempt gets
its own Background Read Context. Native scheduling remains outside this finite
single-call lifetime; this is not a multi-page or aggregate G1 adoption.

The remaining selected local branches are Futures candle validation, history
lookup/range refusals, unsupported one-way setting/private-credential/prepare
refusals; Spot unsupported candle interval, order-book query, leverage null
result, account-setting refusal and batch/amend/cancel validation errors. Valid
signed requests, private account/WS, nonce domains, queued mutations and their
journal adoption remain open. Futures paged candle HTTP/response completion stays
held for G1/Plan/history completion. Existing managed Spot OHLC is preserved.
No signing, account, wire, financial reducer, socket or capability edits belong
in this slice. The open #1798 Spot checksum/book claim remains untouched.

## Native behavior to preserve

Futures catalogue uses /derivatives/api/v3/instruments on production/demo host;
its existing malformed/unlisted row handling, partial catalogue retention and
retry stalls stay exact. First usable catalogue still connects public, starts
account and requests tickers in the same native order. Futures ticker read uses
/derivatives/api/v3/tickers, live_tickers overlay, captured ticker_revision and
per-symbol ticker_sources to avoid replacing newer WS values. Store publication
and every streamed row stage require exact current Background delivery.

Spot AssetPairs and Ticker stay on https://api.kraken.com/0/public with the same
result/error codecs, canonical symbol routes, ticker reseed/balance valuation,
live overlay and startup sequence. The native public sender already has a
20000 ms timeout; retain that bound from admission before Prepare/Bind. Futures
adds the same explicit 20000 ms attempt policy; document this caller-visible
change. No auth evidence, signer, retry credit or money outcome is introduced.

Discovery booleans and existing Backoff remain native work/census state, not
Context authority. Admission/queue failure, preparation refusal, timeout, invalid
body and transport failure must finish the exact Background Context and execute
the existing busy-clear/backoff cleanup once. A failed/expired attempt cannot
strand busy state or publish late data. Success finishes the exact Context before
starting unrelated native follow-up work. No command notice for background work.
No new generation counter, scheduler, shared engine hook or per-venue timeout.

Every selected local branch consumes genuine initial command entry, uses its
native Read or Mutation operation, and completes through done_context or
fail_context with the same JSON/message/error fields. Keep explicit existing
NotSent metadata on pre-I/O refusals. command_wait None/Expired goes through
actual reject_unstamped_command/reject_expired_command, without a default cap.
Rejected admission already emitted its one terminal; Revoked has no second
terminal, numeric fallback, queue advance or permission to affect another owner.
Local Read success uses ordinary Read completion; local Mutation success remains
held on #1989. Valid unconverted signed branches do not acquire a Context merely
because they share a validation function with a converted local failure.

## Original REDs and compatibility controls

Commit this plan before dedicated original tests; run those tests against the
unchanged production with the same real transport oracle before source edits.
A fixture may accept existing raw HTTP and actual Prepare/Bind/Claim, but must not
change native response bodies or financial assertions to obtain GREEN.

Original background controls: a late catalogue/ticker body without firing its
timer must not publish; live response is the working control. A held preparation
expires and clears busy state, then an actual native retry timer admits fresh
work and recovers. Exact old request replay cannot publish or clear the newer
attempt's busy state. Preserve partial-market reread, ticker WS-overlay/revision
and Spot wallet revaluation positives. Count assertion failures only, not absent
API, compilation or fixture setup errors.

Local controls exercise selected native validation precedence/text/metadata,
no network/Store effect, genuine invalid-clock admission and settled ID reuse.
Existing native private/history/spot-candle controls remain working evidence for
unconverted paths. Do not claim an original vulnerability for a missing shared
contract, or fabricate a callback/command root to make a fixture fail.

After implementation, prune only the selected occurrences and retain the one
successful-setting fingerprint plus every blocked entry. Shared fingerprints
are occurrence-counted per path; do not remove another branch by coincidence.
Run both checker commands, scoped Kraken native tests, strict Clippy, fmt/bans;
CI supplies feature/replay/WASM/size gates. Independent source, caller and full-map
review are required before ready. Publish a separate small draft after the prior
review clears, integrate current actual landing, and await a train window.

## Selected source map

- `venues/kraken/src/history.rs:357` — `715c868362434911c7a9d556c59d92966e43d911ef11bf5edf375d57b9a0a4f9`: cx.fail( *command, "Kraken does not support caller client order IDs or their lookup", )
- `venues/kraken/src/history.rs:425` — `25843858bf27d1debf93c3d12a22740d349dd9a6471e1121abc59e58e8bf1d33`: cx.fail( *command, "Kraken history range must contain valid milliseconds in ascending order", )
- `venues/kraken/src/lib.rs:190` — `e1fa27a60b5975d6b5d61534567990770bf7acd939fd2bbe1bb6f1d0a69d83f2`: cx.request_streamed( Method::Get, format!("{}/derivatives/api/v3/instruments", self.host()), vec![], ArrayAt::Field("instruments"), Pending::Markets { rows: vec![], failed: false, }, )
- `venues/kraken/src/lib.rs:207` — `6920a8a0e605c8435f4da4f4ba12691439407417c32c29b9a01701a176581561`: cx.request_streamed( Method::Get, format!("{}/derivatives/api/v3/tickers", self.host()), vec![], ArrayAt::Field("tickers"), Pending::Tickers { rows: vec![], failed: false, revision: self.ticker_revision, }, )
- `venues/kraken/src/lib.rs:365` — `3086bfd3dddf1389c250505f93b2e3f224f1b31ebbb4c41902976df96e49cadb`: cx.fail(c, "Unknown Kraken market")
- `venues/kraken/src/lib.rs:371` — `5a5b7fc7c97b592291089a8d86f674baef6620525fef889feccd6133b6e27933`: cx.fail(c, interval.unsupported("Kraken"))
- `venues/kraken/src/lib.rs:380` — `6bf1b00f37754a5b2c7549f71970a36e362b6efd9c9440d5e400c2cb581ce020`: cx.fail(c, "Invalid Kraken candle range")
- `venues/kraken/src/spot/mod.rs:140` — `7ee70c01ddfed95c96a161f33976a8fbd5a8d529cda8bd31a364e2d8161df4ce`: cx.request( Method::Get, format!("https://api.kraken.com/0/public/{path}{params}"), vec![], None, pending, )
- `venues/kraken/src/spot/mod.rs:941` — `bf0c63ade187f2ea61fdb2e369f2baa04ac8745b7d18512e1ea71a3f9955ced9`: cx.fail(command, tf.unsupported("Kraken Spot"))
- `venues/kraken/src/spot/trading.rs:368` — `ae460df304c6f896a1e611c805f9da6a417d4eab82daf7595c0908570f6506f2`: cx.fail(*command, not_sent(e))
- `venues/kraken/src/spot/trading.rs:377` — `ae460df304c6f896a1e611c805f9da6a417d4eab82daf7595c0908570f6506f2`: cx.fail(*command, not_sent(e))
- `venues/kraken/src/spot/trading.rs:381` — `5a9df80195da64cfb0db15e8bd3898f2f4d0651de85f73b81dac78e4196cb1d3`: cx.fail(*command, not_sent("Kraken Spot empty cancel identity"))
- `venues/kraken/src/spot/trading.rs:395` — `5a9df80195da64cfb0db15e8bd3898f2f4d0651de85f73b81dac78e4196cb1d3`: cx.fail(*command, not_sent("Kraken Spot empty cancel identity"))
- `venues/kraken/src/spot/trading.rs:420` — `ea8c9ac0d82c56d77ecb772d1eb46fc2d9f3154891144bfd2fb3aac247c7c42a`: cx.fail( *command, "Kraken Spot order book snapshots use subscriptions", )
- `venues/kraken/src/spot/trading.rs:424` — `35058464be07d81db637fa288630993ec85908b4d75d5e863ad46e8e5a1cb92b`: cx.done(*command, "null".into())
- `venues/kraken/src/spot/trading.rs:429` — `6ab6b41277dbc3bba05c301ce36fbd8f2124f060f0f091ccd3c7053ec7a68520`: cx.fail( *command, not_sent("Kraken Spot does not support this account setting") .kind(ErrorKind::Unsupported), )
- `venues/kraken/src/trading.rs:184` — `153595b6489266ace2fc389c86c71c07fe76b79d5928210ed88b6ffad5f5127e`: cx.fail( *command, not_sent("Kraken Futures only supports one-way positions") .kind(ErrorKind::Unsupported), )
- `venues/kraken/src/trading.rs:190` — `35058464be07d81db637fa288630993ec85908b4d75d5e863ad46e8e5a1cb92b`: cx.done(*command, "null".into())
- `venues/kraken/src/trading.rs:197` — `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5`: cx.fail(id, not_sent(error))
- `venues/kraken/src/trading.rs:200` — `5d0b0c93e4647bbcb1a8083fdb3059600f03febcb10aee2a745e56a833e726e2`: cx.fail( id, not_sent("Kraken trading requires API credentials") .kind(ErrorKind::AuthInvalid), )

## Original checkpoint and focused conversion

Plan 8ccb55a16c3 precedes original control commit b5c45336471. With unchanged
production, the dedicated real-Engine target executes eight assertion REDs and
one compatibility GREEN, zero ignored. Four expired-publication controls each
first pass the live-body case. The other failures are two raw sends before host
preparation and two local terminals bypassing invalid-entry admission. No absent
API, compiler or fixture failure is counted as an original RED.

All nine original controls pass unchanged after conversion. Four added controls
prove expired catalogue retirement plus actual native retry recovery, old-reply
inertness while the next attempt lives, and held-Prepare timeout releasing busy
state for both Futures and Spot. Contexts remain engine-owned; no local owner
registry, retry budget, generation or timer was added. Source/core plan direction
and the HMAC owner's disjoint file window are CLEAR.

Existing fixtures now drive real Prepare/Bind/Claim without changing their wire,
financial, supervision, retry-delay or row assertions. A signature-only internal
ProbeVenue used catalogue Pending as an inert carrier; that is replaced by the
existing unowned candle carrier, with no fake Context or account/signing edit.
The probe still ignores every response and all header/nonce assertions are exact.
Compiler failures from that new required field and obsolete raw-bootstrap fixture
expectations were corrected explicitly, not counted as REDs.

The inventory delta is exactly 17 fingerprints / 19 occurrences across the five
selected maps; fixed-setting success and all blocked entries remain. Native
verification and independent implementation reviews follow on the frozen source.


Focused final native run passes 260 tests across 19 targets, zero failures or
ignored tests. Strict all-target Clippy and package fmt pass. Earlier full-package
attempts exposed obsolete raw-bootstrap fixtures (one public ticker helper, then
socket/trading bootstraps); corrected transport-only fixtures preserve all
assertions. The final successful run supersedes those failures. Shared checker
and bans receipts, independent review and exact-head CI remain separate gates.

## Independent review receipt

At frozen cf12e136de6, source/adversarial, caller and full-map reviews are CLEAR
(read-only, no independent execution claim). Exact scope is 22 paths; all 5577
unowned incoming paths are preserved. The original nine-control file is an exact
byte prefix with four appended tests. Existing native assertions and nonce/header
proofs remain intact. Inventory maps, their retained key/value order and the
held Futures success are independently confirmed. Both checker commands and
bans pass, including the 15 checker controls. Exact-head CI/replay/WASM/size and
bounded core source review remain separate pending gates.

Core cleared cf12e136de6. Exact ad59 CI builds for Futures and Spot are both
1,144,447 bytes for web and Node, leaving 199,041 bytes below the family budget.
Futures replay passed. Spot's compiled frame fixture lacked prepareHttp, so its
managed startup failed before the financial assertions. Against the exact CI
artifact, all five original frame controls fail unchanged; installing the shared
recorded preparation helper makes all five pass. The helper's own test also passes.
No frame assertion, production source or other Spot reply handler changes.
Fresh full CI remains required; the earlier failed Spot run is historical.
