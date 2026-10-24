# TxFlow immediate local command custody

Plan first, 2026-10-24. Owner codex-hcmq3f. Fresh branch
`feat/rust-txflow-local-command-context` starts at actual landing
`9330c11bdea731368cb1254fa3dc068d11380cdd`. This phase adds only this plan,
its index row and original-source tests. No production or inventory change.

## Inventory and scope

The canonical checker inventory contains 25 TxFlow occurrences at this base.
Source re-verification selects 15 immediate completion occurrences; eventual
adoption leaves 10. The initial 16-site candidate included one branch already
preempted by the engine account-capability guard. This
slice needs no unlanded engine API, G1, G2, G2b, G2c or signer admission.

| Source at the base | Selected occurrences | Native branches |
| --- | ---: | --- |
| history.rs:255-316 | 6 | Unsupported public trades / client-ID lookup, missing testnet account, invalid history bounds, unknown/ambiguous symbol, filtered empty income history. |
| lib.rs:517-535 | 3 | Candle market, interval and range validation. |
| trading.rs:289-332 | 6 | Cached leverage success/failure, unsupported book/STP, unauthenticated write, invalid built write. |

The shared FetchStpMode/SetStpMode lexical occurrence represents two operations:
Fetch is Read, Set is Mutation. The declaration follows the genuine command,
never its error text, HTTP method or credential availability. Other local reads
remain Read even when native errors already contain Unsupported/NotSent fields;
retain that metadata. Writes use keyless Mutation admission and exact failure,
not Read casting. No successful Mutation is selected.

Ten held occurrences include the unreachable native FetchAccount refusal and
raw physical issuance (account, public info/exchange
and trading) and physical candle/write completion. Valid history retains native
CursorHistory/Completion, payloads, pagination and failure handling. This plan
does not grant retained callbacks a fresh command entry, change accepted IDs,
add signing custody, or alter account publication and reconciliation.

## Genuine entry and native behavior

At this base, on_command alone calls fetch_candles, query and trade_command.
No response, timer, queue advance, socket or history continuation re-enters those
helpers. The selected branches terminate before physical I/O. Pure builders
retain their native validation order; valid built commands still enter the
existing trading queue and pump. Keep metadata/asset lookup, wallet selection,
confirmed setting cache, public/private flags and testnet checks unchanged.

The eventual helper will use the existing command_wait cause-aware rejection:
None -> reject_unstamped_command; Expired -> reject_expired_command. Remaining
uses admit_http_command with the genuine Operation, credential None, retry
Never, no pacing and explicit 20,000 ms policy, immediately followed by
Read done_context/fail_context or Mutation fail_context. There is no HTTP,
credential, synthetic attempt/fact, numeric terminal fallback or local registry.
Revoked yields no authority; Rejected already produced its one shared terminal.
Local results keep exact JSON and error metadata. This is immediate ownership,
not a new physical timeout or renewed caller lifetime.

## Original-source REDs and working controls

Commit the plan before new controls; commit controls before running them on
unchanged production. Tests use real Engine Event::Command entry at NaN to
exercise the typed InvalidInput refusal requirement. Pin every selected branch,
including separate Fetch/Set STP cases, cached leverage, empty filtered income,
all three candle refusals, authentication and a genuine build validation error.
A compile error or absent API is not an assertion RED.

Finite-clock working controls keep exact native errors and typed outcomes,
confirmed leverage JSON, empty-history JSON, settled same-ID reuse, and no HTTP
or timer action. Bootstrap through existing native metadata/account fixtures;
do not synthesize private state, fake identity provenance or replace transport
assertions. Preserve existing native history/trading/candle/account tests.

After the original receipt, source remains held until the requested count and
scope are reported. Eventual production keeps all original assertions and prunes
exactly 15 occurrences; all ten held and foreign inventory entries remain.
Scoped native tests, strict Clippy, formatting, request-context checks and full
bans precede draft review. CI must report TxFlow family size/headroom; no profile,
budget or assertion adjustment substitutes for the gate. No READY or merge claim
from this plan/RED phase.

## Source coordination

Frozen CPU slice #2031 at `06eb6851450` owns ticker projection in lib.rs and
Instrument hints in wire.rs plus its dedicated tests. Its owner confirms no
local completion/history/trading/candle hold. Assume #2031 lands first, then
plain-merge its actual landing; preserve its catalogue hint rebuild and live
public ticker application byte-for-byte. No wire.rs edit is needed here.
Managed socket and account production are outside this slice. Signal any source
composition to its owner before freezing; do not resolve a semantic overlap by
changing either contract. Current plan/tests run on 9330 as requested.

## Original fixture correction before receipt

The first scoped run exposed a fixture provenance mistake: Engine::handle
intercepts FetchAccount before venue entry because TxFlow declares account
lookup Unsupported (engine.rs:3805). Its native trading.rs arm cannot be reached
through genuine entry, so it is held rather than inventing a wrapper/capability
or treating its NaN assertion as an adoption RED. Keep the shared pre-venue
unsupported reply as a GREEN. The finite-clock control now expects that exact
public behavior. The other 16 cases still target genuine entry (two STP
operations share one lexical occurrence). Do not claim the first run as
17 genuine REDs or an all-GREEN native control receipt.
