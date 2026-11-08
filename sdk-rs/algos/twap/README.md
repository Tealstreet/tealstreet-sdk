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
WritePlan adoption were reverted in new commits. The shared helper remains
available from the stacked #3348 branch, but this TWAP path does not adopt it:
its stop-on-failure contract would change legacy continuation. Existing chaser
cancellation and compensating market-remainder writes are retained.

The factory parity regressions record algorithm commands, not physical HTTP
requests: BingX's unbounded maximum remains usable, a restored [3, 1] plan with
minimum 2 releases the prefix, and rejected/unknown second children still
allow a distinct third child. Existing controls cover all-valid schedules and
chaser timeout/failure market fallbacks.
