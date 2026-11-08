# Bybit write plans

Ordinary and Hyro mutation dispatch uses `safe-cex-engine::write_plan`. The owned
payload keeps exact JSON plus create rows and immutable native precision,
quantity caps and published minimum-notional rules. The pure callback checks all
rows, mutation values and batch counts before admission. Existing complete lot
formatting, conservation checks, option price/IV rules and live release vetoes
still apply. Fixed-price non-reduce-only creates honor minimum notional; market
and BBO execution prices retain their existing semantics.

Each physical call has one journal index, with consecutive logical elements for
native create/cancel batch rows. Read-only partial-stop proofs consume no mutation
index. Dispatch marks the index pending before signing/admission. A local veto
settles NotSent; a lost released reply stays Unknown. Strict native row decoding
and the existing engine classifier supply each result before aggregate reduction.
Unproven HTTP seals copy the same journal through `UnprovenEvidence::write_plan`.

New native creations use Placed with their returned IDs. Accepted effects without
new native IDs use Applied:

| Effect | Correlation |
| --- | --- |
| Cancel, partial-stop cancel | Existing native target ID |
| Cancel-all | Native category, symbol/coin and order-filter scope |
| Amend, attached-leg clear | Existing native parent/target ID |
| Trading-stop, partial-stop replacement | Native category, symbol and position index |
| Leverage, margin mode, position mode | Native symbol/coin scope or account setting endpoint |

Targets never become Placed identities. Existing successful command return values
remain compatible, including amend's existing target ID. Applied effects survive
later failures and prevent an entire partially applied intent being called unsent.

A terminal refusal stops every unissued mutation. There is no batch-to-singles
fallback, automatic cancel, partial-stop restore or rollback. A failed replacement
reports its applied cancel and its own classified result, retaining protection
warnings and native reconciliation. Existing engine-granted fresh-signature
retries remain within an unsettled call; ambiguous or terminal failures never
retry. Custody, signing, nonce and physical transport ownership stay unchanged.

See the [split-plan invariant](https://github.com/Tealstreet/tealstreet-sdk/blob/docs/split-plan-invariant/docs/plans/SPLIT_PLAN_INVARIANT.md).
