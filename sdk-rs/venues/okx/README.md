# OKX legacy execution parity

The retained TypeScript adapter is the authority for trading rules. This change
adds outcome reporting without introducing whole-plan minimum, native cap or
wire-grid validation. Existing formatters and native execution owners decide
which writes are sent, retried, continued or compensated.

| Path | Legacy TS behavior | Rust scope |
| --- | --- | --- |
| Positive split tail below native minSz | `okx.exchange.ts:2266-2284` emits every positive remainder without a minimum-lot check. | Preserve the tail; no new minimum gate. |
| Row after the 20-row batch boundary | `okx.exchange.ts:2290-2315` sends batches serially. A later native refusal can follow an accepted batch. | Preserve native batch dispatch and report known outcomes. |
| Separate TP/SL children | `okx.exchange.ts:1758-1763,2181-2200,2318-2343` sends entry batches before full-input children, without native min/cap checks. | Match entry batching, child amounts and stable algo priority; remove the added child gate. |
| Ordinary amount/max/precision | `okx.exchange.ts:1751-1755,2226-2242` formats all ordinary rows and refuses nonfinite/nonpositive values before writes. | Retain matching preflight checks. |
| Opening conditional trigger/amount | `okx.exchange.ts:1759-1763,2162-2172` formats all algos and refuses a missing trigger or invalid amount before writes. | Retain matching preflight checks. |

Source paths above are under
`packages/safe-cex/src/__tests__/reference/okx/`; the same methods were checked
in the pre-move adapter at `4a3af89e6^`. The shared child derivation is
`packages/safe-cex/src/exchanges/base.ts:1446-1478`.

Known legacy issue: positive subminimum tails, later native rows and protective
children can partially execute before native rejection. This PR records that
behavior; it does not impose a new order-planning policy.

`safe_cex_engine::write_plan` supplies optional indexed placement error facts.
Its passive journal adds no native validity rules, and reporting failures cannot
block execution. A proven signature rejection retains its existing authorized resend
while the same logical row awaits its final outcome. Ambiguous released writes
stay unknown under the existing engine lifecycle. Cancel and compensating flows
keep their existing scheduling and native evidence; the optional placement
journal is omitted for those flows. Removed pre-existing put-back and resend
code is restored. The legacy TS snapshot proves protective cancel/re-place, not a
separate original-stop put-back algorithm.

After failure, normal and algo placement series stop, as TS's serial placement
helpers throw. Cancellation row errors inside a code-0 envelope continue; every
nonzero envelope stops, matching `okx.api.ts:77-82`. Regular and algo cancellation
tests cover both paths; WritePlan does not control dispatch.

Sequence pins require all entry batches before SL/trailing/TP algos. Children
retain the original input amount, so a child refusal leaves the accepted entry
prefix and stops later algos. Per-lot entry/SL/TP interleaving is not TS parity.

The mixed-algo pin asserts trailing priority only. TS `okx.exchange.ts:2203-2208`
sends `closeFraction=1`, omits `sz`, and uses the input price as `callbackSpread`.
Inherited Rust behavior sends native `sz` and a distance from the last price;
this remains a documented deviation, covered separately as a native control.

This PR remains stacked on #3348; re-merge master after that dependency lands.

Stop-after-Unknown is a recorded parity exception (Sean/daxx, 2026-11-08).
When a native response leaves a write unproven, the rest of that action stays
unsent, including signature-looking replies and protective replacements. The
terminal preserves accepted IDs and indexed placement facts, identifies the
unconfirmed request, and says `The outcome is unknown. Check Open Orders`.
The venue emits one `order.outcome_unknown` event with the command, accepted
count and unconfirmed request. Late acknowledgements cannot restart the action
or emit another terminal/event. Proven signature refusals and definite native
cancel-row refusals retain their existing resend/continuation behavior.

The nine path pins live in `tests/support/unknown_exceptions.rs` and
`tests/protective_amend.rs`, covering
split tails, later ordinary batches, TP/SL children, standalone algos, valid
ordinary and conditional runtime writes, normal/algo cancellations,
signature-looking ambiguous replies, and protective cancel/re-place. Their
comments cite the legacy TS continuation each exception overrides. Placement
pins check placed, Unknown and unsent journal elements; cancellations retain
the deliberate optional-placement-journal omission and the engine's call facts.

KNOWN-ISSUE (deferred to #3359): engine-sealed Unknown telemetry. HTTP
408/429/5xx pins still assert the stop, Unknown outcome and the engine's canonical
text; only their telemetry assertion is deferred. No shared engine code is
changed to add this venue companion event.
