# Bybit legacy parity and outcome reporting

Legacy TypeScript owns the write behavior. Ordinary non-option orders have no
new whole-command price, precision or minimum-notional gate. Existing capability,
caller-ID and option preflight checks remain. The previous Rust lot normalization
and transport classification stay in their existing owners.

Hyro awaits each order and stops when it fails. Its shared `WritePlan` is a
reporting journal with an accepting callback; it records Placed,
Applied, the classified failure and untouched NotSent rows. Typed envelopes retain
native effect scopes without untyped JSON trees or invented order IDs. Read-only
partial-stop confirmation requests consume no mutation index.

Spot retains its distinct native-rejection stop and caught-provider-failure
continuation. Placement there uses existing engine aggregate reporting, since a
permanently stopping journal would change the latter behavior.

Linear/Inverse batch placement retains the pre-adoption native batch-to-single
fallback and continuation schedule. This path uses its existing engine aggregate
reporting: the shared helper permanently stops after a failure, so enabling it
here would change the legacy write sequence. No new helper API or venue-local
replacement journal is introduced.

| Accepted effect | Correlation |
| --- | --- |
| Cancel, partial-stop cancel | Existing native target ID |
| Cancel-all | Native category, symbol/coin and order-filter scope |
| Amend, attached-leg clear | Existing native parent/target ID |
| Trading-stop, partial-stop replacement | Native category, symbol and position index |
| Leverage, margin mode, position mode | Native symbol/coin scope or account setting endpoint |

Partial TP/SL edits cancel then replace, as TS does. Replacement payloads use the
native default Market execution and omit conditional Limit execution fields for
both Full and Partial modes. A failed replacement does not automatically restore
the old stop: TS has no such catch/restore branch. Protection warnings and native
reconciliation remain. Existing declared batch fallbacks are retained.

Known legacy issue: ordinary missing/zero prices, below-minimum notionals and
other later native refusals may follow accepted rows. Batch rows/chunks may
continue after earlier failures. This PR does not add an atomic-admission policy.

The earlier [split-plan invariant](https://github.com/Tealstreet/tealstreet-sdk/blob/docs/split-plan-invariant/docs/plans/SPLIT_PLAN_INVARIANT.md)
is superseded for this PR by the owner's legacy-parity scope.
