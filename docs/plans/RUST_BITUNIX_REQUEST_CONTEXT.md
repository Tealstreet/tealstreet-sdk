# Bitunix REST RequestContext inventory and migration plan

Owner: codex-9czdjh; overseer: claude-ddi2il.

Snapshot inventory: **23 occurrences / 20 path-hash rows / 4 files**.

## Shared authority contract and review gates

This is a source inventory and migration plan, not a claim of completed adoption.
The snapshot is `fa5d5fb90c46f1c9d2984e1ed9c451b5f8370785`. Paths below are relative
to `sdk-rs/`; line numbers locate that snapshot, and full scanner hashes identify
ratchet rows. A repeated hash has an explicit occurrence count and all source lines.
Conservative macro/field matches are identified separately from dispatch/completion
calls; preserve their assertions and semantics rather than removing them to quiet
the checker. No inventory exemption, numeric completion helper, fabricated Context,
credential, signer, HTTP operation, or replacement venue pager is permitted.

Use [RequestContext](RUST_REQUEST_CONTEXT.md), the existing Binance managed lifecycle,
engine row admission, and the shared history algorithms. A command enters through
the genuine engine command-entry authority before native I/O. The reviewed API must
provide denial before Context issuance; a refusal already completed by the engine
returns immediately. Do not add another legacy `fail` fallback. `Error::venue_error`
from reviewed #1808 converts Context errors; it grants no command-entry authority.
Validate the original completion and exact delivered leg before decoding effects,
Store/event publication, history admission, incompleteness marking, or terminal
completion. Duplicate, superseded, and reused-ID deliveries grant no authority.
An expired success grants no Store/send rights; engine-admitted timeout/retirement
may complete the current original root through generation-only completion authority.
Use generation-bound `done_context`/`fail_context`, never the numeric command ID.

Single-response reads need no speculative Plan. Multi-request work requires the
shared Plan admission/journal owner, including children, retries, original deadline,
partial IDs, and cancellation. Background work needs a retained Scope and engine
session/operation authority, with cleanup before callbacks and late deliveries.
History uses the shared Context-bearing Completion/HistoryRows/CursorHistory bridge;
keep `finish_with`, reject, accepted-only recovery, and the original root. Native
codecs supply native rows/codes only. Move existing error mappings into the shared
classification table; do not add status ladders, outcome aggregation, local retry
policy, effective/unknown demotion, or completeness logic. Signers export reviewed
SignStep recipes with opaque identity; preparation preserves native bytes, static
headers, broker fields, credential role, route, and signing-at-dispatch behavior.

Each implementation area gets a separate PR: failing regression tests in one
commit, source and exact venue-only ratchet shrink in the next. Append landed caller
behavior to RUST_CUTOVER_CALLERS.md, audit/quality evidence to their appendices, and
update this plan/index as appropriate. Recheck every open PR file list immediately
before source/test edits. Do not edit a source/test file claimed by another PR.
Shared docs and the ratchet are shared by design. At integration keep incoming docs
first and this appendix once verbatim; keep incoming inventory entries and reapply
only this venue's actual prune, then regenerate/verify the checker.

Independent source/adversarial, caller, and full-map reviews must establish exact
inventory coverage and answer: “Is this a per-venue fix of a cross-cutting concern?”
The intended answer is no: missing authority goes to the shared owner. Locally use
`CARGO_BUILD_JOBS=2`, focused touched-crate plus Binance tests, strict scoped clippy,
fmt and both request-context checks. CI owns WASM/replay/performance gates. No merge
without overseer window and a clean revert guard. Docs-only inventory commits do
not claim runtime tests or changed caller behavior.

## Scope, collision, and phases

The 23 scanner occurrences comprise 19 REST/completion calls and four conservative
macro matches, including those in files also used by sockets. Open PR #1771 owns entire Cargo.toml, src/account.rs, src/lib.rs,
src/socket_codec.rs, tests/auth_results.rs and tests/socket_lifecycle.rs. Do not edit
those files, including their REST sections. All five physical REST dispatchers are
in claimed account.rs/lib.rs. history.rs/trading.rs and new disjoint REST test files
are available for planning/test-first work, but swapping terminals there cannot
claim a complete managed HTTP migration through blocked legacy dispatchers.

1. Once #1771 lands, or a refreshed open-PR file audit proves the files are no
   longer claimed (or the overseer grants an explicit exception), public markets/
   tickers/candles first.
   Preserve testnet x-simulated-trading=1 using the shared static-header API. Public
   GET secondary egress is host plan data (`readPublic` -> Public read route), per
   credential owner claude-m26ps4; do not add venue routing overrides.
2. Genuine single private requests/settings POSTs (leverage/margin/mode) follow
   command entry and reviewed Bitunix signer. claude-19pt8s owns sign.rs recipe and
   coordinated signed_request/credential seams in account.rs/lib.rs. Existing shared
   primitive is Sha256(Credential) -> Sha256SecretSuffix(Result hex). Consume its
   reviewed SignStep, not a new crypto or venue signing path. Signer plan #1810
   `8433022cfab` reserves no history.rs/trading.rs REST flow.
3. Single fills, order/income history and later trading use shared Context history
   and Plan/journal admission. No proved multi-owner coalescing here; Read.after is
   one command. Do not invent a group dependency. Account snapshots/refresh and
   reconciliation chains require the same shared row/plan authority as other venues.

There is currently no unblocked complete HTTP conversion: source collision is
independent of signer/entry/header prerequisites. Report rather than bypass #1771.

## Caller behavior and regression matrix

Markets/tickers preserve partial-row/full-envelope rules, nonempty readiness,
private initialization before tickers, degraded/retry signals and rate floors.
Candles retain quote volume, seconds, native response order, interval-first refusal,
no new market guard, and distinct explicit-limit versus omitted-limit query bytes.
Private GET selects secondary; mutations and WS authentication select primary.

History rejects unsupported client lookup/public trades before keyless handling;
keyless private results are `[]`. Missing/unknown fill symbol returns `[]` before
finite-time validation. Do not add range-order validation. Fills is single page;
order overlap/cursor is bounded at 50 pages and income skip at 100. Preserve raw
progress before mapping, descending order, order limits applied after paging and
existing income kind/limit handling. A later-page failure returns accepted rows only
with an owned incompleteness marker before completion. Preparation refusal remains
strict, and missing/nonadvancing cursors/caps retain shared completeness semantics.

Trading tests preserve IDs, batch-wide preflight, partial IDs, independent continuation,
optimistic cleanup, fill races, definitive-refusal-only recovery and no retry of
unknown writes. Replace venue aggregate/effective demotion with engine CommandWrites
journal facts only after the shared cores; preserve all matched recovery assertions.
Test stale response/timer, duplicate, command-ID reuse, preparation refusal, cancellation,
reconnect cleanup and invalid envelopes alongside byte-exact native route/signing proof.

## Exact scanner inventory and engine-path map

| Source at snapshot | SHA-256 scanner key | Count | Classification and destination |
| --- | --- | ---: | --- |
| `venues/bitunix/src/account.rs:337` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/account.rs:335` | `37e060689327c1ad79d592ea1a9079e30ef97cf58ffc0b65967b779b8d5b97fa` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/account.rs:562` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/history.rs:223,264` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 2 | call: History preflight/empty result -> Read Context; request/history progression waits shared dispatcher, signer and Context history bridge. |
| `venues/bitunix/src/history.rs:268` | `6ae09f4ddf0da52a0d4557bbbbdf7683b02309fd5b83052f95902ca84e4e8a69` | 1 | call: History preflight/empty result -> Read Context; request/history progression waits shared dispatcher, signer and Context history bridge. |
| `venues/bitunix/src/history.rs:219` | `e6976f28ef1c6418ff7a12ae23db6119dc6a593aaa5ef6e736e97acde6ba1477` | 1 | call: History preflight/empty result -> Read Context; request/history progression waits shared dispatcher, signer and Context history bridge. |
| `venues/bitunix/src/history.rs:212` | `ea50153d8d70fa0cb55007393aadf62c3666dbb5e2f9d6c5d64bedb72d3299e5` | 1 | call: History preflight/empty result -> Read Context; request/history progression waits shared dispatcher, signer and Context history bridge. |
| `venues/bitunix/src/lib.rs:151` | `2550ad03895c0f6a051fbcdc81d13f2b6bf8e622cec5166292c130c02207a2d8` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/lib.rs:139` | `8e9c6f91ed3af3c6827d9d5f84fc2f7e39dd4919fac482cbe69b6cd5384d316e` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/lib.rs:980` | `934e56fa93ceed83a6210d98437ca0fc4586e45e66981b562f8660a11afa6bc4` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/lib.rs:715` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/lib.rs:716,823` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/lib.rs:997` | `d3cd5e4283cb70e8237d488a2da4477a9f90315cbeb8a47e5ce612849d98cff3` | 1 | call: #1771 collision: defer entire file; REST managed lifecycle + headers/signer/Context and native row admission after collision clearance. |
| `venues/bitunix/src/trading.rs:1852` | `03c9a7632009d78b3353fb0a558c46b45901784ec35f68bf18794743ec39c8fc` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitunix/src/trading.rs:289` | `5c8327b0c32fec41a26bebe2be6242f01f42dd559db6c6ed0d3a44593daa63b9` | 1 | call: Trading preflight/terminal -> command entry + Plan/CommandWrites journal; waits managed signed dispatcher, preserve partial IDs. |
| `venues/bitunix/src/trading.rs:1331` | `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2` | 1 | call: Trading preflight/terminal -> command entry + Plan/CommandWrites journal; waits managed signed dispatcher, preserve partial IDs. |
| `venues/bitunix/src/trading.rs:1219` | `67b7e6451565bda29255c0f41f9f46bfc8309ec5998b99f6e286840574190e1c` | 1 | call: Trading preflight/terminal -> command entry + Plan/CommandWrites journal; waits managed signed dispatcher, preserve partial IDs. |
| `venues/bitunix/src/trading.rs:1360` | `c75a7e2cea8f3da1f10de9b8aa940fb4fcd3c57ddbccaf72faa83ab9c6e6fcdb` | 1 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitunix/src/trading.rs:1300,1342` | `e844b5bdfef1fbcdb97ebe211443d5dd47ca3b0ddd4f3c4d63512b2db77c6f81` | 2 | macro: Conservative macro match; retain assertion/branch/codec behavior during the associated lifecycle/history/Plan migration. |
| `venues/bitunix/src/trading.rs:1221` | `f8b99f320746bee1eaad4962337f49532934f311df03900770ee8305e576db60` | 1 | call: Trading preflight/terminal -> command entry + Plan/CommandWrites journal; waits managed signed dispatcher, preserve partial IDs. |

This table exhausts this venue’s current ratchet rows. Planning changes no source,
tests, capability flags, caller behavior, or inventory entries.

## Paired error-classification adoption proof

Bitunix adoption must fix the shared error-classification spec #1814 row 33:
private read 401/403/404 must not be flattened to network failure, and native
10007 must not disagree as authInvalid on writes versus invalidInput on reads. Consume the shared
classifier with decoded native-code precedence and its read-auth result, after
its reviewed API pin. Add RED then GREEN coverage; do not add a per-venue status
ladder or broaden resend/unknown-write recovery.
