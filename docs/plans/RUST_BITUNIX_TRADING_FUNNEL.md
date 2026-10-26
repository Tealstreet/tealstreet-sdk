# Bitunix trading funnel

Plan CLEAR (dazzy3, `7649f028389`, with conditions (a)-(c) folded). Owner: claude-4yjlw0 (HMAC lane; overseer ruling (a): signer migration, then trading). Overseer: claude-ddi2il. Reviewer: one of claude-fe7lgm or claude-dazzy3 (speed mode). Standard: better than TypeScript. One PR (plan, originals and source).

Base: `feat/rust-rewrite` after #2190, the Bitunix dispatch signer. Template: Bitrue B1 #2173 and B2 #2201, and BloFin #2157.

## Scope

Every remaining legacy Bitunix request leaves `cx.request`/`request_streamed`: account.rs 2 and trading.rs 8 request sites, down to 0. Signing is already at its floor (#2190).

- **Reads** (`signed_request`: account axes, symbol settings, history pages) move to managed authority:
  - account axes and settings become Background `SelectedRead`s on the read key, with native's backoff and retry kept;
  - history pages are legs of the command's caller Read root, as in Bitrue B1.
- **Trading writes** move onto a Preflight root per command and one streamed serial Plan per job.

## Trading shape

- **Entry.** Each command opens `begin_preflight(WriteAfterReads, read: Primary, write: Primary)` before validation; a validation refusal is `fail_preflight` (NotSent).
  - Keyless keeps the local refusal ("Bitunix credentials required"), as Bitrue B2.
  - **Concurrency (Q1, CLEAR).** Jobs stay concurrent, keyed by command, with no FIFO: a FIFO would hold a protective cancel behind a batch place's 1000 ms chunk gaps. Each job owns its Preflight and Plan, and the engine's per-order lock (`guard_order_mutations`) refuses a second mutation of the same order, which native allowed.
    - **(a) The lock lasts for the job.** It covers the whole serial Plan, continuations included. The amend fallback (cancel, ReconcileReplacement, replacement) and the "position does not exist" re-place never release the order between calls.
    - **(b) Settings mutations.** Leverage and margin serialize against each other per symbol, and position mode per account, through the existing engine lock (refuse or chain, whichever the engine already offers), with no new venue queue. Placements stay concurrent with them, as native.
    - **(c) Fresh snapshots only.** Concurrent jobs' prerequisite positions, orders and TP/SL snapshots apply through the shared codecs under the managed-read generation rule Bitrue's `apply_orders` uses. A late response never overwrites a newer Store.
- **Prerequisite reads.** A placement that must resolve a position or TP/SL reads positions, then orders, then TP/SL, through account reads attributed to the command (`Prepare` stages 0–2).
  - These become the serial Plan's first Read calls, Primary-signed in-job, as Bitrue D3.
  - Each read applies through the account codecs: the existing snapshot application is split out, as Bitrue's `apply_orders`.
  - A failed prerequisite is NotSent ("Bitunix preflight reconciliation failed") before any write.
- **Serial calls.** Place, the TP/SL families, batch place, cancel batches, amend, leverage, margin and mode.
  - Writes are `SerialCall { Mutation, 20 s, attempts: 2 }`; Bitunix's `10007` "Sign signature error" (200 envelope; safe-cex's signature-rejection table) is the one RefusedResend.
  - The ReconcileReplacement and order-detail GETs are Read calls.
  - Native's 1000 ms between batch-place chunks becomes `continue_serial_plan_after(1000)`.
- **Recoveries stay in the job.**
  - **"Position does not exist" on a single place.** A definitive refusal re-runs the prerequisite reads and re-places once (native `recover`), as continuation calls on the same Plan after the refused write's DATA. The journal keeps the refused fact.
  - **"Order does not support this operation" on an amend.** Falls back to cancel, ReconcileReplacement and replacement, again as continuations.
- **Write facts.** `error::aggregate`, `effective`, `append` and the partial-ID bookkeeping are deleted. Native codes move into a cited `CodeTable` (Bitunix error-code docs), and accepted IDs ride the journal. Batch ACKs use `http_write_batch`. Native text stays the selected or noted message.
- **Reconcile.** `reconcile_uncertain` triggers Primary order snapshots (`PrimaryRead { PostUnknownOrderRead }`); `capture_http_reconciliation` uses the endpoint path.

## Original controls (RED first)

1. Account axis and settings reads are managed Background reads on the read key, with native backoff kept.
2. A history page is a leg of the caller root; a late page is refused.
3. A validation refusal is the command's single NotSent; keyless stays local.
4. A placement's prerequisite reads are Primary-signed Read calls; a failed one is NotSent with no write.
5. Batch-place chunks are 1000 ms apart under one Plan; a refused chunk keeps earlier accepted IDs as partial.
6. "Position does not exist" re-reads and re-places once; a second refusal ends the job with the first fact kept.
7. The amend fallback runs as continuations; the replacement appears only after the cancel is proven.
8. A cancel batch's partial ACK keeps accepted IDs; an unknown leg projects Unknown.
9. Two commands on the same order: the second is refused by the order lock before any write, including one that arrives between an amend's cancel and its replacement (a).
9b. A second leverage, margin or mode change on the same scope is serialized or refused, while a placement proceeds concurrently (b).
9c. Two jobs whose prerequisite reads return out of order: the older snapshot never overwrites the newer (c).
9d. A cancel is not held behind another job's 1000 ms batch-place gap.
10. Inventory: Bitunix request 10 to 0.

## Decisions

- **Q1 (dazzy3, CLEAR):** native concurrency under job-scoped order locks, with conditions (a)-(c).
- 10007 is the only RefusedResend: it is refused before execution, and the provider draws a fresh nonce on re-sign.
