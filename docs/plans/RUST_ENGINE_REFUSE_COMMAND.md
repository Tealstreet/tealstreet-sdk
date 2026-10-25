# Engine local command refusal helper

Status: PLAN. No source before S0 (#2103) and #2114 land, and before the
overseer has seen the size numbers below. Owner claude-fe7lgm; reviewer
dazzy3. Queued from the #2114 review.

## Problem

A venue that refuses a command locally, before any I/O, must still settle the
genuine command entry, so that a full owner registry or an invalid clock yields
the engine's one typed admission refusal. Every adopted venue does this by
hand:

```rust
let Ok(context) = cx.admit_http_command(Policy {
    operation: Operation::Mutation,
    credential: None,
    timeout_ms: 20_000,
    pacing: None,
    retry: RetryPolicy::Never,
}) else {
    return;
};
let _ = cx.fail_context(&context, error);
```

A scan of `feat/rust-rewrite` at `ba6fb191239` finds this exact shape 58
times in 32 venue source files across 16 venues; #2114 adds a 59th. The shape is a
literal `Policy` with no credential, no pacing, `RetryPolicy::Never`, and
`fail_context` with no request between. Bitget and OKX already wrap it in
their own `refuse_entry`, with a credential.

This is a cross-cutting concern done per venue. Each copy can drift, and every
copy also hides two silent paths:
- a `Revoked` admission (a non-genuine entry) returns with the command never
  settled;
- `let _` drops a settle failure.

## API

One method on `Cx`, next to `admit_http_command` in
`request_context_entry.rs`:

```rust
/// Settle this genuine command entry with a local pre-I/O refusal.
pub fn refuse_command(&mut self, operation: Operation, error: impl Into<VenueError>)
```

- It admits exactly the policy the copies use today: `credential: None`,
  `timeout_ms: 20_000`, `pacing: None`, `RetryPolicy::Never`. It then settles
  with `fail_context(&context, error)`.
- If admission is `Rejected`, the engine has already emitted the typed
  refusal, so the method returns.
- A `Revoked` admission is a caller bug (a non-genuine entry). Today it hangs
  the command silently; the helper adds `debug_assert!` on it and on the
  settle result.
- It leaves the error's outcome alone. Mutation callers keep passing
  `NotSent`, as they do now, under a `debug_assert` that a Mutation refusal is
  `NotSent`. No caller-visible change.
- It is not `#[inline]`: one body per family, so each further site costs a
  call instead of a copy.

## Call sites

- **P2, the queued set.** These carry the review that started this:
  - Aster `trading.rs` `start_trade`;
  - Backpack `history.rs` `reject_history_query`, `lib.rs` (unsupported
    interval) and `trading.rs` `reject_fixed_setting`;
  - Lighter `trading.rs` `refuse` from #2114, and Lighter's 7 `history.rs`
    refusals.
- **P3, the rest of the literal shape, one commit per family:**
  - GRVT 12;
  - WEEX 9;
  - Hyperliquid 6;
  - Paradex 5;
  - MEXC 3, Phemex 3;
  - Orderly 2, BingX 1 (`history.rs` `local_result`);
  - WOO, TxFlow, KuCoin spot, Coinbase and BreakoutProp 1 each.
  - The exact list is the first commit's scan output, re-run on the tip at
    that time.
- **Out of scope:**
  - The 78 admissions built from a venue policy helper, mostly with a real
    read credential. They carry a different policy on purpose (metadata roots,
    signed reads); they are surveyed, not migrated.
  - BingX `lib.rs` `refuse_candle_query`, which admits with
    `timeout_ms: remaining`. The timeout feeds admission's deadline check, so
    it keeps its own policy unless a control shows it is inert.
  - Bitget and OKX `refuse_entry`, which pass the primary credential. They
    move only if a control shows the credential does not change the
    capacity-refusal terminal; otherwise they stay as they are.

## Size (estimates; measured before any source lands)

- Today each copy inlines a `Policy` literal, the admission match and the
  settle call: roughly 80–150 bytes of wasm per site. A helper call passes an
  operation and an error: roughly 20–40 bytes.
- **Per family:**
  - A one-site family moves its body into the helper: neutral, within about
    ±50 bytes.
  - Each further site saves roughly 60–100 bytes.
  - So Lighter (8) is about −0.4 to −0.7 KB, WEEX (9) about the same, GRVT
    (12) about −0.7 to −1.0 KB.
  - Single-site families are about 0.
- **Combined:** the helper instantiates once per family, so the saving is
  roughly the sum of the family savings: about −2.5 to −4.5 KB.
- **Gate:** every migration commit reports its family's exact before/after
  bytes and the combined. A family whose measured delta exceeds +50 bytes goes
  to the overseer before it lands, rather than being made to fit.

## Tests

- **Engine REDs, then the helper:**
  - At a genuine entry with free capacity: one terminal with the venue's exact
    message, kind and outcome, and no I/O.
  - At full owner capacity and with an invalid clock: the typed admission
    refusal.
  - At a non-genuine entry (a timer or response callback): the debug assertion
    fires, where today the refusal is silently dropped.
- **Per family:** the existing local-refusal controls stay green unchanged
  (Aster #2027, Backpack, Lighter #2114, and the others' entry tests). A
  migration commit adds no new behaviour, so it adds no RED.

## Order

1. S0 (#2103) lands. The helper sits in engine code that S0 rewrites, and S1's
   `CoreCx` later makes it non-generic for free.
2. #2114 lands, so Lighter's `refuse` exists to migrate.
3. This plan gets a CLEAR, and the overseer reviews the size estimate.
4. P1: the helper plus its engine REDs and controls.
5. P2: the queued set (Aster, Backpack, Lighter), with measured sizes.
6. P3: the remaining literal-shape families, one commit each, each
   shrink-only or neutral.
