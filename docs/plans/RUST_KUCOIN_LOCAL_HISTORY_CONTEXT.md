# KuCoin futures local history command authority

Base: actual landing `46c4dbe6ebbca15d8a5c7eeff044005ab34d7979`.
This plan precedes tests and production. Refreshed open-PR file lists show no
collision in `kucoin/src/history.rs`, `funding.rs` or the new test. Private HMAC
ownership was notified; no signer, lib.rs, Spot or shared-engine edit is needed.

## Selection and exact inventory

This is the largest currently unblocked coherent venue area: 16 direct numeric
terminals across futures history/funding, versus five Bitunix local history,
five Bitrue history preflights and four futures candle expressions. Bitunix
lib/account/socket files remain #1771's. BloFin history/lib/tests remain #1865's.
Private request helpers, paired/multipage work, group/background roots, Session
and signer-dependent paths retain their recorded shared prerequisites. Spot
setting no-ops keep their successful capability contract; mutation terminal
conversion is not a Read shortcut.

Full KuCoin inventory before this slice: 80 occurrences, 70 hash rows, nine
files. This slice removes 16 occurrences/12 hash rows: history13->5 occurrences
(11->5 rows), funding9->1 (7->1 rows). Full venue becomes64 occurrences/58 rows
in nine files. All other venue/file maps remain incoming verbatim.

| Source before edit | Exact fingerprint | Native branch |
| --- | --- | --- |
| `history.rs:256` | `557a3c547f8fbe25d2435df46a5dcdfedc8cbc48a7bd84f93300dd188bfb60bd` | cx.fail(command, "Client ID query is not supported by this venue"); |
| `history.rs:290` | `760402c5deb4b7e740454abfc537c556868ba8de0d9474196df4bc18882ccdc3` | cx.fail(command, "Invalid KuCoin order ID"); |
| `history.rs:294` | `7492685f19344bbb5f71aaa9ce5ddd5a6c8334ec01afea4272d430e24a8c8ff7` | cx.fail(command, "KuCoin credentials required"); |
| `history.rs:300` | `2014ae71f12da2dbe77247605b6ed232a347804065ec697883c6453be4d684b2` | cx.fail(command, format!("Unknown KuCoin market {s}")); |
| `history.rs:306` | `fe59f0c530fb432f64301445ac4166bffdf59e67617abe41b8cd709527d3f06d` | cx.fail(command, "Invalid KuCoin query time range"); |
| `history.rs:310` | `fe59f0c530fb432f64301445ac4166bffdf59e67617abe41b8cd709527d3f06d` | cx.fail(command, "Invalid KuCoin query time range"); |
| `history.rs:332` | `fe59f0c530fb432f64301445ac4166bffdf59e67617abe41b8cd709527d3f06d` | cx.fail(command, "Invalid KuCoin query time range"); |
| `history.rs:370` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | empty result |
| `funding.rs:112` | `b794452f2a30bb8d7962980aa180699a68c7d0b92f6cba189f507f886f6dfca9` | cx.fail(command, "Unsupported KuCoin funding query"); |
| `funding.rs:116` | `b794452f2a30bb8d7962980aa180699a68c7d0b92f6cba189f507f886f6dfca9` | cx.fail(command, "Unsupported KuCoin funding query"); |
| `funding.rs:120` | `7492685f19344bbb5f71aaa9ce5ddd5a6c8334ec01afea4272d430e24a8c8ff7` | cx.fail(command, "KuCoin credentials required"); |
| `funding.rs:124` | `e0e5973853e07ba0ab695de8b9f063ba7b1e66042c92d7e34545cabefa62b30b` | cx.fail(command, "KuCoin funding history requires a symbol"); |
| `funding.rs:128` | `4403f13e4ebee41090fae68af15a7e2b9dc365acd15180931a19617f04e019ff` | cx.fail(command, format!("Unknown KuCoin market {symbol}")); |
| `funding.rs:134` | `9a60e91368234580b182f6a992676102fae962a3e073ccdc5c5af671782ab1ef` | cx.fail(command, "Invalid KuCoin funding time range"); |
| `funding.rs:138` | `9a60e91368234580b182f6a992676102fae962a3e073ccdc5c5af671782ab1ef` | cx.fail(command, "Invalid KuCoin funding time range"); |
| `funding.rs:143` | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | empty result |

The remaining six history/funding scanner entries are conservative embedded
macros: five history assertions whose tokens include done or request.url, and
one funding format! invocation with request.api_symbol. Keep all six exact;
none is exempted. Native legacy signed_streamed, Completion and paging source
remains separately held even where the scanner does not count it.
All other KuCoin inventory groups remain: account3, identity2, lib10,
secondary_tests10, Spot mod16, Spot trading12, futures trading5. The original
full venue map and held paths remain in RUST_KUCOIN_REQUEST_CONTEXT.md.

## Implementation shape and caller contract

Declare one `history::local_read_policy()` with Read, no credential, 20,000ms,
no pacing and Never retry. Each existing synchronous branch directly calls
`cx.admit_http_command(policy)` in the genuine initial Query callback before
returning a result. Both admission errors return immediately: Rejected already
emitted the sole typed refusal and Revoked grants no rights. No numeric fallback,
Context fabrication, private transport credential, new owner registry, timer or
local completion wrapper. A no-I/O branch truthfully declares credential None.

Use original root `done_context` for both zero-limit empty results and
`fail_context` for unsupported client lookup, defensive funding type/kind guards,
missing credentials, invalid order ID, missing/unknown symbols and invalid ranges.
Keep every original message/branch precedence and companion-free timeline.
Unsupported errors gain Unsupported/NotSent; local validation/absent credential
errors gain InvalidInput/NotSent. Admission denial keeps the shared reason and
has no read financial outcome. No event, HTTP, socket, history marker or Store
mutation is introduced. The zero-limit history owner is constructed then dropped
as before; do not convert CursorHistory/Completion or acquire a root on a valid
signed/public paging fallthrough. No private data is read by these local paths.

Three guards are defensive: the two funding type/kind guards and history
windows(None). The sole public dispatcher routes
only Income/FUNDING_FEE into funding_command; validated integer bounds plus
clipping mathematically cap history windows at MAX_REQUESTS. These three guards
have no direct production entry witness. Migrate them through the same checked local terminal API, but do
not claim native original-RED coverage for unreachable branches. Reachable
branches and genuine shared-entry denial are the required production witnesses.

## Tests before source, audits and gates

Add `tests/local_history_context.rs`, separate tests commit, using a genuine
Engine<Kucoin wrapper> that delegates real command handling. Seed only markets;
no fabricated Context or manual callback origin. A prior no-ID callback fills
all256 existing Background roots; assert saturation, make repeated rejected
queries across every reachable local terminal and verify exactly one shared
admission denial/no I/O, then explicitly retire roots and recover at one clock.
Use non-finite raw command times to prove invalid-policy/time admission replaces
the native result, and fixed-clock repetitions to prove terminal retirement.

Original assertion REDs: unsupported/validation typed metadata; native no-I/O
results bypassing capacity; invalid-clock bypass. Compatibility GREENs: original
message/precedence, zero-limit shape, no error companion/history marker/Store
mutation and unchanged valid signed paging path/request bytes. Cover client ID,
invalid order before credentials, missing credentials before symbol/range,
unknown symbol, non-integral/negative/nonfinite/reversed/greater-than-MAX_SAFE_INTEGER range,
funding delegation and missing symbol, both zero-limit endpoints. A long valid
range is a clipped paging compatibility fallthrough, not a local refusal. Valid private
paging retains legacy scheduling/headers/routes and no managed Prepare or owner.

After source, require all new assertions GREEN plus existing KuCoin and Binance
native tests (both KuCoin feature modes), strict scoped Clippy/fmt, bans, exact
own-only manifest prune and check-request-context.py plus its tests. Three
independent reviews: source/adversarial, caller and full-map; all must be CLEAN.
Plan and phase audits precede source. Update CALLERS/AUDITS/QUALITY and README;
architecture gate: shared initial command authority, no per-venue mechanism for
a cross-cutting concern. Full CI, explicit train slot, genuine latest landing,
fresh separate preservation guard then head-matched merge. This closes local
terminals only, not private history, signing or whole-venue zero.

## Executed phase receipt

Plan reviews corrected the survivor classification and separated three defensive
guards from thirteen dispatcher-reachable terminals before production. Tests-first
`84d84e1c27f`, corrected fixture `e9693e10536` delegates KuCoin's native mutation
guard. The corrected original source ran five assertion RED/six compatibility
GREEN controls; the same frozen tests pass all eleven after adoption. No failure
was manufactured for a defensive branch. Full KuCoin/Binance all-feature native
run passes586 tests across43 targets; KuCoin feature-off passes173 across17.
Strict all-target/all-feature KuCoin Clippy, scoped fmt, bans and both request
context checks (14 checker regressions) pass. Source/adversarial and caller
reviews are CLEAN; final full-map review and normal PR CI are separate gates.
