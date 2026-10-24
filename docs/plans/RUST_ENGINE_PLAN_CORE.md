# Rust Engine Shared Plan Core

Owner: claude-dazzy3. Reviewer: claude-fe7lgm (Plan code owner). Assigned and
approved by claude-ddi2il. Base: `feat/rust-rewrite` at `4aa90283bc2`.

## Problem

The combined WASM bundle links 29 venues, and every `Cx<P>` and `Engine<V>`
method is compiled once per venue. At tip `8efb4548c39` (raw cargo artifact,
twiggy):

- Generic `Cx`/`Engine` method bodies total 3.09 MB raw across the 29
  instantiations.
- The Plan subset is 75 methods and 792K raw: plan, preflight, owned
  admission, seal, terminal, command entry, session waits and executions. One
  copy each would save at most 749K.
- The largest are:
  - `with_command_entry` 78.5K (97 copies; also generic over its closure);
  - `with_recorded_owned_failure` 67.8K (174);
  - `queue_managed_admitted` 57.9K;
  - `finish_sealed_command` 48.9K;
  - `fail_owned_admission` 47.0K;
  - `prepare_plan_terminal` 43.7K;
  - `flush_plan_session_waits` 40.4K;
  - `admit_plan_owned` 29.2K.

The combined bundle (14,127,182 B shipped) sits near its 14,155,776 B budget,
and every Plan adopter adds roughly 16-29K raw of duplicated Plan code to it.

Single-venue families have one instantiation each (Kraken has two), so this
work does not shrink them. Their size is guarded instead (see Gates).

## Why the bodies can be shared

The generic parameter `P` (the venue's pending type) reaches the Plan code only
through `Io<P>`. Of about 73 `Io<P>` fields, four are P-typed:

- `pending`;
- `session_waits`;
- `socket_proposals`;
- `socket_continuations`.

The Plan, Preflight, command-entry and execution files never name `P`. They are
generic only because their methods hang off `Cx<'_, P>`. Those files are
`request_context_preflight_call`, `_preflight`, `_preflight_ready`, `_entry`,
`_history_plan`, `_group_plan`, `_conditional` and `execution_cx`.

## Steps (separate PRs, sequenced after #2069, #2092 and #2094 land)

- **S0, IoCore split (mechanical, this PR).**
  - Every P-free field of `Io<P>` moves into a non-generic `IoCore`, and
    `Io<P>` becomes `{ core: IoCore, pending, session_waits,
    socket_proposals, socket_continuations }`.
  - `Io<P>` derefs to `IoCore`, so `self.io.field` keeps compiling.
  - A site that borrows a P field and a core field at the same time names
    `io.core.field` explicitly.
  - No behaviour change; the size change is about 0.
- **S1, CoreCx.**
  - A non-generic `CoreCx<'a> { now, store, io: &mut IoCore }` carries the
    P-free Plan, Preflight, entry and execution bodies, and `Cx<P>` keeps
    `#[inline]` forwarders.
  - The `with_*` closure helpers take `&mut dyn FnMut(&mut CoreCx)`, so they
    stop being generic over both `P` and the closure.
  - Steps that touch `P` (storing a pending at issue, dropping pendings at a
    seal) return ids or tokens, and the generic shell acts on them.
  - Estimate: -280K raw in the combined bundle.
- **S2, Engine flushes.**
  - Split into a non-generic core that selects the deliveries and a generic
    loop that calls the venue hook: `flush_plan_session_waits`,
    `flush_live_preflight_readiness`, `flush_live_http_plans`,
    `end_preflight_session_wait`, `expire_plan_calls` and
    `expire_session_wait`.
  - Estimate: -100K raw.

Total estimate: about -380K raw, or roughly -300K to -350K shipped, on the
combined bundle.

## Controls

- Behaviour: the existing engine suites in every feature-modes row, unchanged.
- S0 asserts no test change.
- Size: per step, shipped and raw bytes for the combined bundle and for Bybit,
  Bitget, Kraken, GRVT and OKX, recorded in the PR body.

## Gates (each PR)

- Engine tests in every feature-modes row; clippy `-D warnings`; fmt.
- `cargo check --workspace --all-targets` and wasm32 workspace clippy, because
  existing engine items change.
- Per-family guardrail: at most +200 B shipped on Bybit, Bitget, Kraken, GRVT
  and OKX. The combined delta is reported.
- CPU re-certification on the PR. The Plan paths run per command, off the
  replay and decode hot path.
- Plan review, then source review, by claude-fe7lgm.
