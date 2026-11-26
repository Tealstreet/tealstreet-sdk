# Lighter grouped protection sizing

Grouped limit TP/SL orders check the parent minimums and child triggers before
one OTO/OTOCO submission. Each protective child has native amount zero and closes
the parent fill; it is not checked as a standalone minimum-quantity/notional lot.
This matches the [legacy grouped-order builder](https://github.com/Tealstreet/tealstreet-sdk/blob/431ba3dd4aa8191ee27ef7a76fba30ae290b1ff4/packages/safe-cex/src/__tests__/reference/lighter/lighter.exchange.ts#L2514-L2562).

Market minimum exemptions and remainder-based lot allocation retain legacy TS
behavior. Ordinary non-market lots and native limit batches retain their existing
minimum checks before submission.

An Unknown placement halts remaining writes for that action. Its failure includes
"We couldn’t confirm this change. Check your open orders before trying again.", accepted order indexes and the
indexes whose placement is uncertain. The engine's mutation journal retains the
accepted prefix. For venue-reported uncertainty, `order.outcome_unknown` telemetry
snapshots placed, unknown and unsent indexes from that journal and the dispatcher
queue; engine-owned HTTP failures retain engine journal/telemetry. This is the approved
stop-after-UNKNOWN parity exception; uncertain placements are never replayed.
Acknowledged group legs remain uncertain until individually observed, including
when a later write fails.

Protective edits use a genuine MARK receipt no older than five seconds. Missing
or stale proof triggers one unsigned exchangeStats preflight read under the
initiating owner, bounded to 20 seconds with no retry. Only an accepted target
market MARK row grants proof; Last, Index, frame arrival and delayed envelope
completion cannot renew it. Every final send, including replacement, nonce
retry and put-back, retains the same pure veto with no I/O inside it.

Frame state v2 adds the source's exact validated standalone native parameters;
receivers validate them against the ingested order without importing any MARK
receipt. Empty provenance still exports the existing 11-byte v1 form; new readers
accept both versions. A v1 source grants no protective-edit provenance. Deploy
matching source and receiver builds to use v2; older readers reject the new
version. See [protective edits](../../../../docs/plans/callers/lighter-protective-edits.md).

A resolved frame receiver cannot rebind its account, including address selectors.
Each protective edit retains its initiating account through the public read and
final sends; foreign metadata retires pending edit bindings, even if A returns.

## Builder-fee exemptions

Attributed Lighter orders carry Tealstreet's builder fee (`builder()` in
`src/trading.rs`). `Options::builder_exemption` can remove it for one approved
account, and only with a Tealstreet-signed token (`src/builder_exemption.rs`):

- Token `lbx1.<account>.<expires unix s>.<key id>.<signature>`; the Ed25519
  signature covers a domain-separated payload naming the network (`mainnet` or
  `robinhood`), the account index, the expiry and the key id.
- Accepted public keys are the compiled-in `KEYS` list, empty until Tealstreet
  adds its offline key; `shipped_keys_are_pinned` fails on any unreviewed change.
- Honoured only when the token verifies, names the account being traded,
  has not expired and expires within 400 days. Anything else keeps the fee, the
  tier check and the approval flow unchanged (fail closed).
- Decided once per job, when it first reaches the fee gate: the tier check, the
  approval read and every signed transaction of that job follow that decision,
  even if the token expires or comes into range mid-job.
- Tests drive the exempt path with a fixed-seed key that only the
  `builder-exemption-test-key` feature accepts. Only this crate's own
  dev-dependency enables it, and a WASM build with it fails to compile.
- Plumbed as `lighterBuilderExemption` in the Rust registry options only; it is
  not part of the public TS `ExchangeOptions` and is never set from the terminal.
- Offline tooling: `cargo run -p safe-cex-venue-lighter --example builder_exemption -- keygen|sign|check`.
  The seed file is the signing key and must stay offline.

Note: `lighterIntegratorAccountIndex` (public, mutable) overrides the integrator
on every network, matching the legacy TS resolver. Whether production should
honour it on mainnet is a separate product decision.

## Placement proof (AGT-45)

A `sendTx` acknowledgement only says the sequencer queued the transaction; the
order can still be canceled at execution (margin, slippage, liquidity) or the
transaction can fail. Every ordinary placement therefore records an engine
pending placement and parks its Plan for at most 10 seconds while background
reads prove it: authenticated `accountOrders` by client index (up to 20 per
read), and `tx?by=hash` for a placement whose row has not appeared. An `open` or
`filled` row, a fill, an untriggered stop's `pending`, or a resting order's later
`canceled`/`-expired`/`-oco`/`-child`/`-liquidation` resolves it; a reason-qualified pre-rest
cancellation (`canceled-margin-not-allowed`, `-post-only`, `-reduce-only`,
`-position-not-allowed`, `-too-much-slippage`, `-not-enough-liquidity`,
`-self-trade`, `-invalid-balance`), an unfilled market/IOC order that ended
(`canceled`, `canceled-expired`, ...), or a failed transaction (`status` 0) refuses it
`Rejected` with the venue reason and withdraws the optimistic row. Only exact
statuses resolve: `in-progress`, an unlisted spelling and a non-trigger `pending`
keep polling. A mix of refused and live lots, or no proof by the bound, is
Unknown; nothing is resent.
Reads back off 250 ms to 2 s. A proven placement keeps the legacy 100 ms pace
before the command's next write, which continues from the settled call. Grouped
TP/SL keep their stream proof and a guarded put-back keeps its `RestoreProof`.

## Plain limit amends

An unguarded limit edit is one native `ModifyOrder` (tx 17) of the resting order,
with trigger price 0 (the venue's nil trigger) and the left-of-total amount. A
refusal leaves the original untouched. Edits that cannot be one modify (split
lots) refuse before any write; there is no cancel-then-replace fallback.

The tx 17 acknowledgement is not the result (AGT-77): a size-up the venue
refuses for margin is queued, then fails at execution and leaves the order
unchanged. The ACK records an engine pending receipt (the engine allows one for
an Amend intent as well as a Place) and parks the Plan under the same 10-second
bound while `tx?by=hash` proves it. An executed transaction (status 2-5) completes
the amend; a failed one (status 0) refuses it `Rejected`, "Lighter order not
changed: <venue text>"; anything else is Unknown by the bound. Guarded
protective modifies keep their own proofs.

**A failed or unproven transaction resyncs the nonce.** A failed tx did not
take its nonce, so continuing the local sequence made the next write fail
`21104 invalid nonce`. A status-0 transaction (placement or modify) and a proof
that expires invalidate the counter, so the next write reads `nextNonce` first.
An order-status refusal (`canceled-margin-not-allowed`) executed its transaction
and keeps the sequence.

## Stop-entry brackets (AGT-115)

Legs attach only to a limit/market entry that is not reduce-only: grouped OTO/OTOCO for a
limit, separate trigger steps after a market. Any other entry with a non-zero leg, and a
reduce-only limit/market with one, used to be placed without them; `prepare_orders` now
refuses the whole command before any signing with the engine's shared `stop_entry_legs_refusal` copy, or its own reduce-only copy.
