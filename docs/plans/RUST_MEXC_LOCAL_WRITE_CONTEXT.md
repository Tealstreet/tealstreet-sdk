# MEXC local write refusal Contexts

Owner codex-ih35js; fresh branch `feat/rust-mexc-local-write-context` from
actual `ba3d7da1d3aaba26d6c40d6615fdc467d6ec185d` (only #1984 docs since
actual caller-root landing6a6845c4491). Plan first, then original REDs; no source
or inventory conversion is claimed at this checkpoint.

## Exact remaining map and selected slice

MEXC has21 fingerprints/21 occurrences. Largest coherent local group is3F/3O:
two reachable pre-I/O Mutation refusals plus one provably unreachable duplicate
Err terminal. Projected total18F/18O, trading7F/7O→4F/4O. This is two converted
behavioral branches and one orphan removal, not three physical calls. Fresh
open-PR file lists show no MEXC source/test collision.

| Path | Fingerprint | Count | Disposition |
| --- | --- | ---: | --- |
| `venues/mexc/src/account.rs` | `3a512e8472fbb7eb5034b94f2172f6b72ec7152336d77e97e71f1ba0e041e668` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/account.rs` | `9957739c32728439a0b5b0cbabfd9b7c77479ca2e08a5c8718213e4ddbcdfc7b` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/account.rs` | `9baacdfca9e0cc474c05532c9e1e6137179f1860fb215900cb3875863da3c055` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/account.rs` | `9ea183eac0882301887d9c9e446a1252e61945a7839d2dc49235be9fa053891b` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/account.rs` | `b44d970931e9cb5f96e525d6b46444a2710921aeb72bce9c635f43c9dd7c8359` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/account.rs` | `b7dc68beffbce5993cd986ca76deaba009d76529c9a08dd7cba43c5f61aa74e8` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/history.rs` | `7d9792a0676bccdf20b899bb81c8f19bdf33402d56972457af968a5c4f3f433c` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/lib.rs` | `2cb4c59f38cca507a15b20d1ace303afd27defc1b4fe0aba50dddcbcb1f9c37c` | 1 | Separately workable unsigned background discovery/depth; outside this local Mutation slice |
| `venues/mexc/src/lib.rs` | `a1d7eb3cdf7f4e938351efa132ee68dac79fc9e6e26db557c0ffd67282e9ae14` | 1 | Separately workable unsigned background discovery/depth; outside this local Mutation slice |
| `venues/mexc/src/lib.rs` | `e7ec3b5f8c170e9a5887721f2eedda084ecf9f5a5eba8d1e899957c63019e0bb` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/secondary_tests.rs` | `01822703e05fca2ea5984588dabf8d7c3aa8138e93501ae6ead58982e25492e3` | 1 | Conservative private-path test witness retained with signer work |
| `venues/mexc/src/secondary_tests.rs` | `f758cbc2754c2b39d2f81d56901f6920f66dcc953a7fe7faa9b5c8f2a5b46c90` | 1 | Conservative private-path test witness retained with signer work |
| `venues/mexc/src/trading.rs` | `06eea309fa05c237f809ccb4c45df7a36798cfb0bcb6ace2f22b4b3d7d6f0ef3` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/trading.rs` | `4310c56cdb4207e078977e5ecfb03800099cc6b8559b1e4cd4c0ef98cc4de727` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/trading.rs` | `5f4315dcd81d8ae6ab0638ddd9864e3fc4326a5911e4df86004080095feaaa15` | 1 | Reachable local missing credential: genuine Mutation entry and Context failure |
| `venues/mexc/src/trading.rs` | `ae3209100adb7617d3a60abc05d13961f6b17901cc02551743ff77b75e18b37f` | 1 | Unreachable Err arm after the earlier unconditional Err return: remove duplicate terminal |
| `venues/mexc/src/trading.rs` | `bb2384c4fe0775f69956644df4cda9966a8f10c980910e36d89210625de5771c` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |
| `venues/mexc/src/trading.rs` | `c090aa594c62e2e9e6909a7328a9019893aeb82f169ff44b63e01cd3132bc052` | 1 | Reachable local step-construction error: genuine Mutation entry, Context diagnostic + failure |
| `venues/mexc/src/trading.rs` | `c1caaf5a8609b4dad0bef6ecd21d19b958817f15852a993214e7669494790435` | 1 | Held private send/async completion or coupled conservative witness: no reviewed MEXC dispatch recipe; no local signer wrapper |

The earlier full venue inventory plan RUST_MEXC_REQUEST_CONTEXT.md supplies the
normalized expressions; this table rechecks every actual key. The unsigned
background2O remains separately workable. No signed wire, pacing, queued jobs,
retry scheduler, identity provider, account or history adoption is included.

## Native order and shared admission

trade_command computes its native command-specific step result first. A step
construction/validation Err currently emits command_error then the same error
with NotSent and returns. It precedes the missing-key refusal, so a keyless bad
command keeps the validation error. An Ok step result without credentials emits
'MEXC trading needs an API key', AuthInvalid/NotSent, with no diagnostic. The
later Err match arm is unreachable because every Err already returned.

For either reachable refusal, directly call Cx::admit_http_command with Mutation,
credentialNone,20000ms,Never,pacingNone. No venue issuer/finalizer wrapper. Admit
before any diagnostic or terminal effect, then use command_error_context for
the first branch and fail_context for the original error projection. Rejected
already emits exactly one typed failure; Revoked returns with no fallback. No
Context is issued for a successful native command: it retains the existing
legacy queue/signing behavior and numeric async terminals, untouched. Remove the
redundant unreachable Err terminal without inventing a third behavioral path.

All commands that reach these two shared refusal branches are Mutations:
place/cancel/amend/settings; FetchLeverage and history are handled separately in
lib.rs, while unhandled trade_command variants return before these branches.
Inspect preflight side effects (including optional UUID entropy in earlier valid
placement steps) and pin actual entry eligibility; never repair revoked custody
with a numeric terminal or mint an owner after opaque I/O. If a reachable refusal
cannot retain genuine Fresh entry, report the exact source path as a core blocker.

Native error messages, types/outcomes, validation-before-key precedence and valid
queue/dispatch expressions stay exact. Invalid clock or owner capacity instead
returns shared InvalidInput/Internal with NotSent and no diagnostic/HTTP/timer/
Store change, releasing only its own engine order-lock custody. Missing-key
AuthInvalid comes from a credential-free local owner, so the already-landed
shared custody rule supplies no auth-health evidence; document that caller-visible
health difference rather than reproducing legacy untracked credential counting.

## Original RED controls and positives

Use the real Engine<MexcFutures>, not a local fake issuer or callback wrapper.
Existing trusted startup fixtures provide markets; actual managed book/candle
commands fill256 peer owners with real preparation tokens. Test each reachable
refusal at invalid clocks and full capacity, with every peer preparation still
live, unchanged Store snapshot, no peer terminal, and same-ID recovery after one
real peer preparation failure frees its slot. Exercise keyless and keyed invalid
leverage/missing-market validation, invalid-time None/NotSent projection, and
missing-key position mode. Preserve the native shared duplicate-ID diagnostic
and original owner liveness; more than256 sequential synchronous refusals must
release their slots/locks.

Working positives pin exact command_error-plus-terminal ordering for validation,
missing-key message/AuthInvalid/NotSent with no diagnostic, validation taking
precedence over missing key, supported private Mode still producing its exact
legacy signed POST/body, valid commands still queueing natively, and both bounds
recovering after the actual owner terminal. Do not count an unreachable-branch
control or a fixture/setup failure as a behavioral RED. Pin preparation-capacity
refusal after native preflight where a valid earlier placement consumed entropy,
if such a command can subsequently enter the local Err/keyless branch.

## Gates and callers

Commit plan and original assertion REDs separately before source. Add CALLERS,
AUDITS and QUALITY for the two native local refusals, typed initial admission,
diagnostic precedence and credential-free auth-health behavior. Preserve every
existing financial/identity/signature assertion. Run MEXC plus Binance native,
scoped strict all-target/all-feature Clippy/fmt, both request-context checks and
full bans with jobs2. Exact removal-only manifest3F/3O; all unowned maps exact.
Independent source/adversarial, caller and full-map reviews must clear before
freezing; CI owns WASM/replay/CPU/size/memory. Draft PR; no merge without GO.

Architecture gate: No. Direct existing shared Mutation entry/refusal, diagnostic
and terminal authority; no local journal, classifier, timer, credential-counting,
issuer/finalizer wrapper or numeric fallback. Remote write adoption stays held
for its complete signer/queue/journal migration.

## Original checkpoint

All three independent plan gates are CLEAR. The test-only actual-Engine target
request_context_local_writes compiles and executes13 controls on byte-exact
actual production: nine behavioral assertion REDs plus four native positives,
zero ignored. Four input shapes cover the two reachable branches, including
valid UUID entropy before a later validation error or missing-key refusal.
Both capacity and invalid-clock controls assert the entropy prefix was executed.
Actual managed book peers fill256 owners; liveness and recovery are pinned.

An initial diagnostic assertion mistook native CommandError for a generic Event;
that fixture mistake is corrected before counting REDs. The four working native
controls pin exact error/diagnostic order,300 synchronous ID reuses, duplicate
managed-owner isolation and the exact accepted legacy private Mode POST/body.
The ninth RED records the intentional shared credential-free auth-health change.
No production/manifest edit belongs to this checkpoint.
