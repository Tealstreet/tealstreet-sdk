# Create-order write plans

Ordinary and Hyro create dispatch uses `safe-cex-engine::write_plan`. The owned
payload keeps the exact JSON body plus row values and immutable market precision,
quantity caps and published minimum-notional rules. The pure callback checks all
rows and batch counts before admission. Existing complete lot formatting,
conservation checks, option price/IV rules and live release vetoes still apply.
Minimum notional is checked when the create carries a fixed price; reduce-only
orders retain their native exemption. BBO and market orders do not carry an exact
execution price to validate as a fixed-price notional.

Each physical create has one journal index, with one consecutively indexed
logical element per native row. Attached TP/SL fields belong to that row's
acknowledgement. Dispatch marks the index pending before signing/admission;
a local veto settles it NotSent, while a lost released reply stays Unknown.
Native batch rows are matched and classified before aggregate reduction, so
successful rows survive another row's rejection or unknown result. An unproven
HTTP seal copies the same journal through `UnprovenEvidence::write_plan`.

Any create refusal stops the unsent create suffix. A rejected native batch no
longer falls back to singles. Existing engine-granted fresh-signature retries
remain inside an unsettled call; a terminal failure never retries a create.
There is no automatic cancel or rollback. Explicit caller recovery and existing
custody, signing, nonce and reconciliation ownership remain with the venue.

Accepted effects without a newly created native order ID remain outside this
adoption, as required by the lead's known API-gap exception. Trading-stop,
partial-stop cancel/recreate, ordinary amend, cancel and settings paths retain
their existing lifecycle until the shared result type can represent Applied.
No canceled target or client ID is presented as a newly created native order.

See the [split-plan invariant](https://github.com/Tealstreet/tealstreet-sdk/blob/docs/split-plan-invariant/docs/plans/SPLIT_PLAN_INVARIANT.md).
