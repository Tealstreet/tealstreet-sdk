# OKX legacy execution parity

The retained TypeScript adapter is the authority for trading rules. This change
adds outcome reporting without introducing whole-plan minimum, native cap or
wire-grid validation. Existing formatters and native execution owners decide
which writes are sent, retried, continued or compensated.

| Path | Legacy TS behavior | Rust scope |
| --- | --- | --- |
| Positive split tail below native minSz | `okx.exchange.ts:2266-2284` emits every positive remainder without a minimum-lot check. | Preserve the tail; no new minimum gate. |
| Row after the 20-row batch boundary | `okx.exchange.ts:2290-2315` sends batches serially. A later native refusal can follow an accepted batch. | Preserve native batch dispatch and report known outcomes. |
| Separate TP/SL children | `okx.exchange.ts:1758-1763,2181-2200,2318-2343` formats protection without native min/cap checks. | Remove the added child minimum/cap gate. |
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
Its callback adds no native validity rules, and reporting failures cannot block
execution. A proven signature rejection retains its existing authorized resend
while the same logical row awaits its final outcome. Ambiguous released writes
stay unknown under the existing engine lifecycle. Cancel and compensating flows
keep their existing scheduling and native evidence; the optional placement
journal is omitted for those flows rather than changing their behavior to fit
its fail-stop dispatcher. Removed pre-existing put-back and resend code is
restored. The legacy TS snapshot proves protective cancel/re-place, not a
separate original-stop put-back algorithm.

This PR remains stacked on #3348; re-merge master after that dependency lands.
