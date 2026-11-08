# Split-plan invariant (multi-write orders)

Applies to every native venue path that turns one order intent into more than one write: max-quantity splitting, batch chunking, serial batch rows, attached or separate TP/SL children, scale/TWAP/iceberg children, and cancel-replace.

## 1. Predictable invalidity: zero writes

Before the first write, build the complete write plan and validate every lot, row and child at its exact wire values (the rounded quantity and price that will be sent): min and max quantity, step, tick, min notional at the price used, and any count limit.

If any element is invalid, either rebalance the plan into valid elements (only when the intent allows it), or refuse the whole intent with a typed `notSent` outcome and zero transport requests. Never send the valid prefix and then fail on a known-invalid element.

## 2. Runtime failure after some writes: report, never repair

If a write fails at runtime after earlier writes were sent (venue rejection, network error, ambiguous outcome):

- never resend, and never auto-cancel or roll back placed elements (the duplicate-write rule);
- return a per-element outcome (`placed`, `notSent` or `unknown`);
- never report a partially placed intent as whole.

## Required test shape

Every PR that touches such a path carries both of these tests, and each must fail without the fix:

- **Zero-write refusal.** A plan whose last element is predictably invalid asserts the typed `notSent` refusal and zero transport requests. Use a scripted transport that records every request.
- **Per-element outcomes.** A plan where element N fails at runtime (scripted rejection) asserts that elements before N report `placed`, N reports its failure, later elements report `notSent`, and no resend or cancel request is issued.

Include an all-valid control. The PR body must state whether the defect was live on master.
