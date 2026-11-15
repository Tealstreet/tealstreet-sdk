# Bybit legacy parity and passive outcome reporting

Legacy TypeScript owns write behavior. Ordinary non-option orders have no new
whole-command price, precision or minimum-notional gate. Existing capability,
caller-ID and option preflight checks remain, alongside existing price formatting
and engine-owned custody, signing and physical classification.

Position-mode changes use the single linear/USDT request from both TS adapters,
discarding the requested symbol (`bybitinverse.exchange.ts:2318–2335`). Success
and refusal have no per-coin continuation.

Perpetual lot quantities follow `reference/bybit/bybit.exchange.ts:2718–2731`:
emit `ceil(amount / max)` equal rounded lots, then the rounded remainder. This
preserves TS rounding even when the emitted total differs from the requested
amount: `26 / 5 / 1` emits `[4, 4, 4, 4, 4, 4, 1]`. Spot's separate allocation
change is tracked in PR #3358; its allocation is unchanged in this branch.
The first equal lot and the remainder carry the original TP/SL fields; middle
equal lots omit them, including the last lot when there is no remainder.
Inverse follows `bybitinverse.exchange.ts:1523–1541`: round and clamp the amount
to the native minimum/maximum before allocation. Thus `25.6 / 10` emits `[10]`.

Every ordinary and Hyro mutation has logical indices in the passive shared
`WritePlan::new/record/attach` journal. It holds no payload, admission callback,
next-write state or stop flag. The venue records pending rows as Unknown before
transport, then records classified native results. Read-only cancellation proofs
have no mutation index. Native batch fallback is restricted to master's code 10001
trigger. Its singles
retain the original logical row indices and replace the earlier batch refusal
with the actual fallback fact. Other native refusal codes do not select singles.

After a definite refusal, Spot finishes the current caller order's lots, then
stops before the next caller order. Caught provider refusals retain TS continuation.
Linear/Inverse rows, chunks, fallback singles and special/option orders also
continue after definite refusals according to their TS paths. Hyro serial writes,
cancellation suffixes and single-setting commands stop where TS stops.

Genuine Unknown halts every affected action, including suffixes and batch fallback.
This is the recorded stop-after-UNKNOWN parity exception (Sean/daxx, 2026-11-08):
TS continues in Spot payload catches and Linear/Inverse special/batch catches, but
Rust preserves the duplicate-write fence. Errors retain placed and unknown rows
and the canonical text "The outcome is unknown. Check Open Orders". The journal
reports these decisions without making them.

| Accepted effect | Correlation |
| --- | --- |
| Cancel, partial-stop cancel | Existing native target ID |
| Cancel-all | Native category, symbol/coin and order-filter scope |
| Amend, attached-leg clear | Existing native parent/target ID |
| Trading-stop, partial-stop replacement | Native category, symbol and position index |
| Leverage, margin mode, position mode | Native symbol/coin scope or setting endpoint |

Partial TP/SL edits cancel then replace. Replacement payloads use native default
Market execution and omit conditional Limit fields for Full and Partial modes.
The lead's explicit master-preservation rule retains the guarded put-back after
a definitive replacement refusal. Native standalone lineage, complete peer census,
positive cancellation proof and current protection validation authorize one attempt;
Unknown or an already-gone cancel never does. Put-back preserves the original
native size, trigger source and conditional execution fields. Its passive report
appends an Applied TradingStop or failure row without erasing the refused replacement.

The legacy TS cancel/replace context is `bybit.exchange.ts:3221–3224`, and its batch
catch is at `:3015–3042`. Those TS paths lack this put-back and catch more batch
errors; the reviewed master-preservation rule explicitly governs these two details.
Protection warnings and native reconciliation retain their master behavior.

Attaching changes only optional `elements`; the engine owns aggregate outcome,
partial IDs and retry metadata. Unknown presentation adds the canonical shared
wording in venue-local code. The existing unproven-HTTP evidence hook copies the
journal when the engine seals before venue decoding. Reporting errors emit
diagnostics without refusing or terminating native dispatch.
An HTTP-200 mutation decoded by the venue as Unknown also emits the warning
event `order.outcome_unknown`, correlated by command ID. Its message and the
journal's Unknown row use "The outcome is unknown. Check Open Orders".

Known legacy issue: ordinary missing/zero prices and below-minimum notionals can
follow accepted rows. Native batch errors can be followed by later writes. This
PR adds no atomic-admission policy. The earlier [split-plan invariant](https://github.com/Tealstreet/tealstreet-sdk/blob/docs/split-plan-invariant/docs/plans/SPLIT_PLAN_INVARIANT.md)
is superseded by the owner's legacy-parity and reporting-only decisions.

The engine retains its own canonical text when it seals an HTTP Unknown after
a prior refusal. Venue-authored Unknowns use "The outcome is unknown. Check Open
Orders". Transport pins assert the text produced by the sealing layer, the full
journal, placed-prefix IDs and suppressed suffix. P11's failure-projection fix
preserves later captured row facts when an earlier refusal supplies native details.
