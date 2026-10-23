# Bitunix local history Read completion

Owner: codex-9czdjh; landing snapshot `258c880656f9714f14b498e046b8ee52658e702a`.

## Scope and authority

This slice adopts only the five synchronous no-I/O terminal expressions in
`sdk-rs/venues/bitunix/src/history.rs::query`. The complete venue map remains
[RUST_BITUNIX_REQUEST_CONTEXT.md](RUST_BITUNIX_REQUEST_CONTEXT.md): current
23 occurrences / 20 hash rows / four files. No physical HTTP dispatcher, signed
history continuation, account/socket source, trading, credentials or signer is
changed. Open #1771 owns account.rs/lib.rs and its socket/test files; those whole
files remain untouched. New tests use a disjoint integration-test file.

Each local terminal admits a genuine initial command Read with credential None,
20,000 ms, no pacing and Never retry, then immediately calls the shared
done_context/fail_context and returns. Rejected/Revoked return without a numeric
fallback. No Context is issued on the still-legacy signed paging fallthrough.
The command-entry owner explicitly confirmed that credential None is truthful
for these local branches: it declares no remote credential or routing authority.
This grants no private HTTP, signer, Plan or history-bridge readiness.

No venue completion helper, local finalizer, fabricated HTTP, credential or route
is introduced. This is consumption of the landed entry/completion choke points,
not a per-venue ownership implementation. No new owner registry, timer or deadline
is added. Five direct branches use the shared API; any policy constructor is data
only. Native branch order and checks remain in place.

## Exact inventory delta and caller contract

| Terminal expression in history.rs | Scanner SHA-256 | Count | Result |
| --- | --- | ---: | --- |
| Unsupported client-order lookup | `ea50153d8d70fa0cb55007393aadf62c3666dbb5e2f9d6c5d64bedb72d3299e5` | 1 | Same text; Unsupported / NotSent |
| Unsupported public trades | `e6976f28ef1c6418ff7a12ae23db6119dc6a593aaa5ef6e736e97acde6ba1477` | 1 | Same `Not implemented`; Unsupported / NotSent |
| Keyless private query and missing/unknown fill symbol | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 2 | Same `[]`; successful original Read completion |
| Nonfinite time | `6ae09f4ddf0da52a0d4557bbbbdf7683b02309fd5b83052f95902ca84e4e8a69` | 1 | Same text; InvalidInput / NotSent |

Prune exactly these four rows / five occurrences: **23 to18 occurrences, 20 to16
hash rows, four tothree files**. Preserve all incoming entries and every other
Bitunix row/count, including conservative trading macro matches. The complete
remaining map is account.rs three, lib.rs seven, trading.rs eight occurrences;
all destinations and blockers in the full plan remain open. Removing scanned
history.rs terminals does not complete the still-legacy private history owner.

Client lookup and public trades reject before keyless handling. Keyless private
queries return `[]` before time validation. Missing/unknown fill symbols return
`[]` before finite-time validation. Preserve fills timestamp conversion, limits,
native text and lack of range-order checks; leave valid private HTTP, native query
bytes, paging, incomplete markers and legacy Completion unchanged. Local refusals
keep their companion-free timeline. Admission failures use the engine's typed
reason without another terminal or companion, including invalid clocks/capacity.
Only typed error metadata and genuine authority refusal become caller-visible.

## Tests-first and review gates

Commit regression tests separately before source. Execute unchanged production
to demonstrate assertion failures, then preserve those assertions through GREEN.
Controls cover both unsupported branches keyed/keyless, empty keyless queries,
missing/unknown fills before invalid time, finite-time refusal for valid symbols,
fixed-clock terminal reuse beyond the 256-owner bound, invalid-clock admission,
and genuine registry saturation followed by explicit retirement and same-clock
recovery. Saturation must assert all 256 roots were actually allocated in a prior
no-ID callback; use Direct public-feed policy so SetTickerSymbols reaches the
wrapper. Confirm zero new HTTP/preparation/timer/WS actions for local terminals.
Retain an existing signed history regression as the fallthrough compatibility
control; no source or shared test-support edits for that path.

Before edits refresh open PR file lists. Independently review plan and source for
adversarial authority, caller behavior and complete inventory coverage; report the
architecture-gate answer. Run Bitunix plus Binance native tests, scoped strict
Clippy/fmt, bans and both request-context checks. CI owns WASM/replay/performance.
Append caller/audit/quality evidence and README index in the source PR. Merge only
after green CI, clean independent reviews, current landing integration and a fresh
separate guard in an overseer-granted train slot. The landing-proof pause applies.

## Executed tests-first receipt

Plan `91694673cf1` passed independent source/adversarial, caller and full-map
reviews. Tests-only `c04696781c3` ran on unchanged production: five assertion RED,
three compatibility GREEN. The same eight controls are GREEN with the scoped
source adoption. Capacity fixtures assert all256 genuine roots before attempting
every local branch, preserve them across270 repeated rounds, explicitly retire
them, and recover without advancing time. Invalid-clock tests cover all five
terminal expressions. Fixed-clock reuse covers both empty-result expressions as
well as refusals. The valid private history control still sends its native signed
legacy request and completes a genuine empty page; this is preservation evidence,
not a claim that the private path is managed.

Focused verification: all535 Bitunix/Binance native tests across49 targets passed;
strict scoped Clippy, scoped fmt, bans and both request-context checks (14 checker
regressions) passed. Independent source/adversarial and caller reviews are CLEAN;
full-map review and CI remain separate gates. Work is checkpointed while the owner
prioritizes KuCoin Spot profiling; no merge or whole-history adoption is claimed.

## Current integration receipt

Current landing `2badd1cb0e3` is genuinely integrated at `fe4db6c400c`; source,
tests and original plan bytes remain the reviewed `36a63a95eac` version. Fresh
535-test/49-target Bitunix/Binance native verification, strict scoped Clippy,
fmt, bans and both request-context checks (14 regressions) pass on this current
integration. All three independent source/adversarial, caller and full-map
reviews are CLEAN. Eight intended paths preserve every incoming unowned blob;
#1771 source/test claims remain untouched. Publication and normal CI resume;
merge still requires green evidence and an explicit train slot.
