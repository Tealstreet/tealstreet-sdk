# Shared TWAP legacy parity

TWAP admits and releases one scheduled child at a time. Venue formatting and
validation stay at each native write; there is no new shared full-plan quantity,
price or notional gate. The next distinct child remains scheduled after a
rejected or uncertain placement. An uncertain child is not resent.

The legacy implementation at `c78bed93d6eb4e45a3ec072b7d09fc1529d10a9b` provides
the evidence in `packages/safe-cex/src/webworker/twap/TwapInstance.ts`:

| Path | Verdict | Legacy evidence |
| --- | --- | --- |
| Generated children | TS-partial | Lines 80–114 build raw sizes; 232 shifts one child; 249–256 place it. |
| Quantity, price and notional checks | TS-partial | Lines 117–139 select the due child's amount and live ticker; 167 delegates to the exchange. |
| Restore/resume | TS-partial | `handlers/TwapHandler.ts` lines 60–100 restores state/ownership; instance lines 330–345 arm the next timer. |
| Failed or uncertain child | TS-partial | Lines 255–270 log failure and advance; 273–280 schedule the next child. |
| Chaser fallback | TS-partial | Lines 181–194 retain best-effort market remainders; 213–220 retain timeout/failure actions. |

KNOWN-ISSUE (legacy TS also partial-places): an early child can be accepted
before a later child fails a native rule. Runtime failures intentionally leave
later children scheduled. This matches Sam's parity scope and supersedes this
PR's earlier full-plan claims under the
[split-plan invariant](../../../docs/plans/SPLIT_PLAN_INVARIANT.md).

The new validation gate, native notional sidecars and dispatch-controlling
WritePlan adoption were reverted in new commits. This TWAP path does not use
the shared helper. Lead decision #3355 requires any future helper adoption to
report outcomes without deciding admission or continuation. Existing chaser
cancellation and compensating market-remainder writes are retained.

The factory parity regressions record algorithm commands, not physical HTTP
requests: BingX's unbounded maximum remains usable, a restored [3, 1] plan with
minimum 2 releases the prefix, and rejected/unknown second children still
allow a distinct third child. Existing controls cover all-valid schedules and
chaser timeout/failure market fallbacks.

## After-failure parity

| Path | After a failed element | Legacy evidence at the pinned TS commit |
| --- | --- | --- |
| Fresh scheduled children | TS-continues | `twap/TwapInstance.ts:255-280` logs and schedules the next child. |
| Native quantity/price/notional delegation | TS-continues | `twap/TwapInstance.ts:117-139,167,255-280` delegates the write, catches its refusal, then continues. |
| Restore and resume | TS-continues | `handlers/TwapHandler.ts:95-99` and `twap/TwapInstance.ts:330-345,255-280` reuse the same timer/start path. |
| Runtime rejected or unknown child | TS-continues | `twap/TwapInstance.ts:255-280` catches either failure without vetoing the suffix. |
| Chaser handoff and market remainder | TS-continues | `twap/TwapInstance.ts:142-165,181-194,213-220,255-280` retains compensation and the next child after failure. |

The historical `NEEDS-WRITEPLAN-CONTINUE` entries describe the trial helper at
`a28cf2e28`, whose settlement sealed dispatch. That admission/failure-stop
adoption was removed by `059da70bb`, followed by the parity reverts.
`INHERITED-STOP`: no helper stop or refusal remains in the current TWAP path.
Explicit user termination still stops the schedule, as TS checks at lines
267–269. No replacement journal or helper workaround was introduced.

`tests/after_failure.rs` covers each row, with separate restored/resumed cases,
unknown results, a refused chaser handoff and rejected/unknown compensating
market remainders. Its test executor advances epoch time with Tokio's paused
clock, so both cancellation settlement and subsequent child timers are checked.

## Pinned legacy write sequences

Every parity pin cites the TS file/line and its exact `shared-twap` row in
`~/agent-reports/known-issues-for-sam.txt`. These are recording AccountPort and
chaser-port fixtures; they assert algorithm commands, not physical venue HTTP.

| Parity path | Recording test | Pinned sequence |
| --- | --- | --- |
| Fresh scheduled children | `after_failure::fresh_schedule_continues_after_failed_child` | Three distinct buy/market children of size 1; the second refusal does not remove the third. |
| Native quantity/price/notional delegation | `factory::legacy_parity_bingx_unbounded_max_does_not_gate_the_schedule`; `after_failure::native_rule_delegation_with_unbounded_max_continues_after_refusal` | An unbounded max permits [2, 2]; a native refusal still permits the next distinct child. |
| Restore/resume | `factory::legacy_parity_restored_invalid_tail_is_left_to_each_native_write`; `after_failure::{restored,resumed}_schedule_continues_after_failed_child` | [3, 1] is sent despite minimum 2; resumed/restored [1, 1, 1] still sends the third child after the second fails. |
| Rejected/unknown child | `factory::legacy_parity_{rejected,unknown}_child_does_not_stop_later_children`; `after_failure::unknown_runtime_failure_continues_without_resending_that_child` | The failed child is not resent; the third distinct child is sent, preserving reduce-only and supported client IDs. |
| Chaser handoff/compensating remainder | `after_failure::{chaser_handoff_failure_keeps_later_slices_scheduled,failed_compensating_market_remainder_keeps_next_chaser_scheduled}` | Refused handoff then later chasers; or chase -> cancel that chase -> market remainder -> next chase, even if the remainder is rejected/unknown. |
