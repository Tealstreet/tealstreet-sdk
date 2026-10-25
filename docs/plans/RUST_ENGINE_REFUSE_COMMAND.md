# Engine local command refusal helper

Status: PLAN. No source before S0 (#2103) lands and the overseer has seen the
prototype numbers. Owner claude-fe7lgm; reviewer kuhwxi. Queued from the #2114
review. The overseer approved the shape on 2026-10-25: measure a prototype
first, then write the source in phases.

## Problem

A venue that refuses a command locally, before any I/O, must still settle the
genuine command entry, so that a full owner registry or an invalid clock yields
the engine's one typed admission refusal. Adopted venues do this by hand:

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

On `feat/rust-rewrite` at `b0008964fbb` (after #2114), this exact shape appears
at **37 sites in 10 venues**, listed in the appendix. Bitget and OKX already
wrap a credentialed variant in their own `refuse_entry`.

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
#[inline(never)]
pub fn refuse_command(&mut self, operation: Operation, error: impl Into<VenueError>)
```

- It admits exactly the policy above (`credential: None`, `timeout_ms: 20_000`,
  `pacing: None`, `RetryPolicy::Never`), then settles with
  `fail_context(&context, error)`.
- If admission is `Rejected`, the engine has already emitted the typed
  refusal, so the method returns.
- `#[inline(never)]` keeps one body per family even at opt-level z. `Cx<P>` is
  generic, so each family instantiates it once.
- It adds `debug_assert!` on a `Revoked` admission, on the settle result, and
  on `Operation::Mutation` errors being `NotSent`. These asserts change debug
  and test behaviour, so every site must carry the two proofs under Tests
  before it moves.
- It leaves the error's outcome alone, so nothing changes for callers.

## Call sites

In scope are the 37 strict-shape sites in the appendix:
- Backpack 3;
- GRVT 10;
- Hyperliquid 5;
- Lighter 7 (6 history refusals and #2114's `refuse`);
- MEXC 1;
- Orderly 2;
- Paradex 2;
- WEEX 6;
- WOO 1.

Each P2/P3 commit is checked against the appendix rows it removes, not
against a count.

Out of scope:
- **Non-literal timeouts.** BingX `lib.rs:744` and WEEX `lib.rs:225` admit
  with `timeout_ms: remaining.min(20_000)`, and the timeout feeds admission's
  deadline check. They move only if a control shows the timeout is inert.
- **Credentialed refusals.** Bitget and OKX `refuse_entry` pass the primary
  credential. They move only if a control shows the credential does not change
  the capacity-refusal terminal.
- **Variants the strict scan excludes.** Each is reviewed on its own and is not
  part of these phases:
  - statements between admit and settle: Aster `trading.rs:406`
    (`command_error_context` first), GRVT `trading.rs:1241` and `:1256`,
    Hyperliquid `lib.rs:1488`, MEXC `depth_query.rs:111`, Paradex
    `public.rs:1424` and `:1435`, Phemex `public_candles.rs:18`;
  - other bindings: Paradex `trading.rs:468` (`if let`), KuCoin
    `spot/mod.rs:1625` and MEXC `lib.rs:1242` (match arms).
- **Not refusals.** Excluded from any refuse helper:
  - helpers that settle success too (`done_context`): BingX `history.rs:337`,
    Phemex `history.rs:206`, and the Coinbase, BreakoutProp and TxFlow
    `local_context.rs:21`;
  - Paradex `depth_query.rs:90`, which sends a request.
- **Policy-helper admissions.** The 78 admissions built from a venue policy
  helper, mostly with a real credential, are not this pattern.

## Size

### Estimates

- Today each copy inlines a `Policy` literal, the admission match and the
  settle call: roughly 80–150 bytes of wasm per site. A helper call passes an
  operation and an error: roughly 20–40 bytes.
- A family with several sites shrinks by roughly 60–100 bytes per site beyond
  the first:

  | Family | Sites | Expected change |
  |---|---|---|
  | Lighter | 7 | about −0.4 to −0.6 KB |
  | GRVT | 10 | about −0.6 to −0.9 KB |
  | WEEX | 6 | about −0.3 to −0.5 KB |

- A single-site family (MEXC, WOO) only moves its body into the helper, plus a
  call.
- Combined: about −2 to −3.5 KB, since the helper instantiates once per family.

### Gate

- A multi-site family must not grow at all.
- A single-site family may grow by at most +64 bytes. If one grows more, it
  stays unmigrated until S1's `CoreCx` makes the helper non-generic.
- Any other growth stops the work and goes to the overseer.

### Method

The same as #2109:
- bare tip vs composed (tip + change), built from separate trees with separate
  target dirs;
- the same toolchain: Rust 1.99.0, wasm-bindgen 0.2.129, Binaryen 132,
  `CORES=4`;
- combined plus every touched family.

CI's merge-ref figure includes landing growth, so it is not the gate.

## Tests

- **Engine REDs, then the helper:**
  - At a genuine entry with free capacity: one terminal with the venue's exact
    message, kind and outcome, and no I/O.
  - At full owner capacity and with an invalid clock: the typed admission
    refusal.
- **Per-site proofs, in each P2/P3 commit message:**
  1. **Genuine entry.** Each site is reached only from `on_command` or
     `on_venue_action`; the engine opens a command entry for both
     (`engine.rs:4067`, `:4160`). The commit shows this by a caller trace.
     Example: Hyperliquid `trading/schedule.rs` is reached from
     `on_venue_action` `ScheduleCancel` (`lib.rs:1444`), so it is genuine.
     A site reachable from a timer or response callback does not move. It is
     reported instead, since today it silently drops the refusal.
  2. **NotSent by construction.** Each Mutation site builds its error with the
     venue's `not_sent(..)`. Examples:
     - Hyperliquid `trading.rs:162`/`:180` bind `not_sent(..)` and add only a
       kind;
     - #2114's ten Lighter `refuse` callers all pass `not_sent(..)`.
     A site that cannot show this gets a test, or does not move.
- **Per family:** the existing local-refusal controls stay green unchanged
  (Backpack, Lighter #2114, and the others' entry tests).

## Order

1. #2114 has landed (`b0008964fbb`). S0 (#2103) lands next.
2. kuhwxi CLEARs this plan.
3. **Prototype, local and unpushed:** the helper plus Lighter (7), GRVT (10)
   and WOO (1, the single-site case). Measure each and the combined per the
   method above, and report. The phases go ahead only if every family is
   within the gate. If one grows, stop and report.
4. P1: the helper plus its engine REDs and controls.
5. P2: Backpack and Lighter, with measured sizes.
6. P3: the remaining in-scope families, one commit each, each within the
   gate.

## Appendix: strict-shape sites at `b0008964fbb`

How the list is built:
- The scan covers venue `src/` files only.
- It keeps an `admit_http_command(Policy { .. })` literal only when the literal
  has `credential: None`, `pacing: None`, `RetryPolicy::Never` and
  `timeout_ms: 20_000`.
- The literal must be bound by `let Ok(x) = .. else { return; };` and followed
  directly by `cx.fail_context(&x, ..)`, with no statement between.

This matches kuhwxi's independent scan at `ba6fb191239` (38 sites), plus
#2114's `refuse`, minus the two non-literal timeouts above.

| Site | Operation |
| --- | --- |
| `backpack/src/history.rs:154` | Read |
| `backpack/src/lib.rs:1311` | Read |
| `backpack/src/trading.rs:434` | Mutation |
| `grvt/src/account.rs:1221` | Read |
| `grvt/src/account.rs:1234` | Read |
| `grvt/src/history.rs:371` | Read |
| `grvt/src/history.rs:413` | Read |
| `grvt/src/history.rs:427` | Read |
| `grvt/src/history.rs:444` | Read |
| `grvt/src/history.rs:458` | Read |
| `grvt/src/history.rs:475` | Read |
| `grvt/src/history.rs:489` | Read |
| `grvt/src/lib.rs:339` | (variable) |
| `hyperliquid/src/trading.rs:162` | Mutation |
| `hyperliquid/src/trading.rs:180` | Mutation |
| `hyperliquid/src/trading.rs:362` | Mutation |
| `hyperliquid/src/trading/schedule.rs:28` | Mutation |
| `hyperliquid/src/trading/schedule.rs:47` | Mutation |
| `lighter/src/history.rs:248` | Read |
| `lighter/src/history.rs:313` | Read |
| `lighter/src/history.rs:350` | Read |
| `lighter/src/history.rs:363` | Read |
| `lighter/src/history.rs:379` | Read |
| `lighter/src/history.rs:420` | Read |
| `lighter/src/trading.rs:1402` | Mutation |
| `mexc/src/lib.rs:131` | Read |
| `orderly/src/history.rs:40` | Read |
| `orderly/src/trading.rs:27` | Mutation |
| `paradex/src/history.rs:316` | Read |
| `paradex/src/history.rs:341` | Read |
| `weex/src/history.rs:638` | Read |
| `weex/src/history.rs:661` | Read |
| `weex/src/history.rs:699` | Read |
| `weex/src/history.rs:760` | Read |
| `weex/src/history.rs:774` | Read |
| `weex/src/history.rs:841` | Read |
| `woo/src/history.rs:517` | Read |
