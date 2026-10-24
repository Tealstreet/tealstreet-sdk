# Rust Command Read-Child Delivery

Owner: claude-fe7lgm (Plan owner). Consumer: WOO full order funnel (codex-5c57jl
step map). Reviewers: codex-uufub4, codex-5c57jl. Plan and REDs now; source
after combo-5b (#1896/#1919).

## Problem

WOO's full order funnel is one command with mixed operations:

1. an unsigned Touch pre-read;
2. a pure Build (no I/O);
3. paced Mutation writes;
4. Mode/Leverage GET verification after the writes (attempts 0..2, 250 ms).

The root is the command's Mutation Context root. Each step's physical request
is a child from `http_leg_context(root, policy)`. Steps 1 and 4 are Read
children of a Mutation root.

No validator proves a Read child's DATA delivery under that root:

- `validate_read_http_delivery` requires a read-only command
  (`validate_read_completion_context` refuses `mutation_declared` or `writes`,
  request_context_registry.rs:1459-1464), so it rejects the rightful child;
- `validate_background_read_http_delivery` requires a Background owner;
- `validate_http_context` proves only that the handle is live, not that this
  callback is that child's exact current delivery.

## Design

One new validator, symmetric to `validate_read_http_delivery` but with
command-root custody:

```rust
pub fn validate_command_read_http_delivery(&self, root: &Context, leg: &Context)
    -> Result<(), Error>
```

**Root.** `validate_completion_context(root)`, then the root must be the
command's exact current `context_root` (same equality as
`validate_read_root_context`). Read or Mutation, `mutation_declared` or not.
It refuses:

- Session, Background and Execution (physical) owners, with Invalid;
- a #1919 execution observer, with Revoked (it re-dispatches only through its
  execution);
- any command not in the plain Context phase (Preflight or Plan phases have
  their own validators), with Invalid/Revoked.

**Leg.** `validate_http_context(leg)`, `leg.operation() == Read`,
`leg.owner() == root.owner()`, `leg.deadline() <= root.deadline()`, and
`io.http_delivery.context == *leg` (the exact current delivery). A Mutation
leg's response is refused with Invalid: write outcomes go through
`classify_write` / `http_write_result`, never Read publication. A sibling leg
or a socket or timer callback (no delivery) is refused with Revoked.

**Grants.** Synchronous Read publication only (Store, native parse), exactly
like `validate_read_http_delivery`. No send, retry, resend, completion,
journal or outcome authority.

- A Read child never enrolls in the write journal: `register_attempt` enrolls
  Mutation only.
- The command terminal stays `done_context` / `fail_context` on the root, with
  the existing journal projection. A verification read can't erase a write's
  Unknown or relabel it, and a #2016 inherited fact stays Unknown.

**Failure callbacks.** As with the existing validator, a failure callback
carries delivery ownership: `Ok` proves only ownership, never response data.
Retrying the verification uses the existing `http_retry_context` /
ReadFailure policy under the declared attempt budget (0..2) and pacing; the
validator grants none of that.

## Composition with combo-5b

- **#1896 readiness.** The validator only reads `io.http_delivery`, so it needs
  no readiness custody. Issuing the children already goes through the 5b
  `session_scope` / ready guards; there is no change there.
- **#1919.** Execution observers and physical owners are refused, and a planned
  execution uses `validate_plan_delivery`.
- **#2037 / Bearer.** A bound (Bearer) Read child validates the same way, since
  the token lives only in the packet.
- **#2028.** No new map. Command lookups stay on RandomState `commands`.

## Original controls and REDs

The RED commit adds an inert `validate_command_read_http_delivery` that
returns `Invalid`, so the REDs fail at their assertions, not at compile time.

- C1 (control): a Mutation root, a Read child released and answered DATA, then
  `validate_read_http_delivery(root, leg)` gives `Invalid`.
- C2 (control): the same with `validate_background_read_http_delivery(leg)`
  gives `Invalid`.
- R1: the new validator is `Ok` in that DATA callback. Publication runs once and
  `done_context(root)` seals once.
- R2: a live sibling Read leg of the same root (not the delivered one) gives
  `Revoked`, with no publication.
- R3: the delivered leg is the Mutation write, so validating it as a Read child
  gives `Invalid`.
- R4: a socket callback (no delivery) gives `Revoked`.
- R5: a failure callback gives `Ok` (ownership only), with no publication and no
  terminal.
- R6: a foreign root (another command's root, or a stale generation) gives
  `Revoked`.

Added after codex-5c57jl's contract review:

- R7: DATA at the child's exact physical deadline never reaches a callback.
- R8: a physical leg passed as the root gives `Revoked`.
- R9: a retired (finished) sibling child gives `Revoked`.
- R10: after the root's own terminal, the same pair gives `Revoked`.

These land with source, on the post-5b fixtures:

- R3: a delivered signed Mutation write leg gives `Invalid`.
- R11: a Preflight or Plan-phase command gives Invalid/Revoked.
- R12: a #1919 execution observer gives `Revoked`, and a physical Execution
  owner gives `Invalid`.

Caller contract: a `TimedOut` or `Revoked` from this validator on a live DATA
callback is not a terminal. The venue still owns its exact Pending cleanup
(`finish_http_context` / native job cleanup) and must not return early and
strand a waiting job. Real Timeout, preparation and provider failures keep
their own typed paths.

## Gates

Scoped engine tests (`request_context_read_delivery_tests`), with clippy
`-D warnings` in plans+locks and no-default, and fmt. Source and CI follow the
5b landing.
