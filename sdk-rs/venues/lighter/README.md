# Lighter grouped protection sizing

Grouped limit TP/SL orders check the parent minimums and child triggers before
one OTO/OTOCO submission. Each protective child has native amount zero and closes
the parent fill; it is not checked as a standalone minimum-quantity/notional lot.
This matches the [legacy grouped-order builder](https://github.com/Tealstreet/tealstreet-sdk/blob/431ba3dd4aa8191ee27ef7a76fba30ae290b1ff4/packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts#L2514-L2562).

Market minimum exemptions and remainder-based lot allocation retain legacy TS
behavior. Ordinary non-market lots and native limit batches retain their existing
minimum checks before submission.

An Unknown placement halts remaining writes for that action. Its failure includes
"The outcome is unknown. Check Open Orders", accepted order indexes and the
indexes whose placement is uncertain. The engine's mutation journal retains the
accepted prefix. For venue-reported uncertainty, `order.outcome_unknown` telemetry
snapshots placed, unknown and unsent indexes from that journal and the dispatcher
queue; engine-owned HTTP failures retain engine journal/telemetry. This is the approved
stop-after-UNKNOWN parity exception; uncertain placements are never replayed.
Acknowledged group legs remain uncertain until individually observed, including
when a later write fails.
