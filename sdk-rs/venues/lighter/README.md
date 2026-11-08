# Lighter grouped protection sizing

Grouped limit TP/SL orders check the parent minimums and child triggers before
one OTO/OTOCO submission. Each protective child has native amount zero and closes
the parent fill; it is not checked as a standalone minimum-quantity/notional lot.
This matches the [legacy grouped-order builder](https://github.com/Tealstreet/tealstreet-sdk/blob/431ba3dd4aa8191ee27ef7a76fba30ae290b1ff4/packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts#L2514-L2562).

Market minimum exemptions and remainder-based lot allocation retain legacy TS
behavior. Ordinary non-market lots and native limit batches retain their existing
minimum checks before submission.
