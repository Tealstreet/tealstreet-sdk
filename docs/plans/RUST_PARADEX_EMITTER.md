# Paradex and exchange listener audit repairs

Branch `feat/rust-paradex-emitter`, based on landing `c44f22bd8e`.
Task: `.aimux/plans/paradex-and-emitter.md`; spec/process:
`RUST_REWRITE.md`, `VENUE_PORT_BRIEF.md`, production-impact rule in
`RUST_AUDITS.md`. Full automatic execution, no TS venue deletion.

## Phases

1. Listener isolation: retain TinyEmitter API and delivery semantics, catch each
   subscriber failure, visibly report its cause without recursive failure, and
   prove later subscribers receive the same event for TS and Rust accounts.
2. Public/private data: re-verify blank summary/position numbers and malformed
   SBE handling. Coordinate startup/row tolerance with codex-u1pmrg; avoid
   duplicating its repairs. Keep valid account state and healthy sockets usable.
3. Trading/errors: Error-level failed child protection; continue independent
   placement operations and retain accepted IDs with visible per-order errors;
   preserve venue rejection details; serialize leverage as a JSON integer.
   Preflight remains atomic, failed entries skip their children, ambiguous writes
   never retry. Coordinate bridge error delivery with codex-c6nf4w.
4. Gates/landing: regressions for each surviving finding, independent review at
   each phase; native venue tests/format/Clippy/bans, SDK tests/typecheck/lint,
   WASM build/Clippy and Binance/Paradex private replay. Public output/CPU/memory
   gates run in CI on committed fixtures; no local CPU admission wait. Open PR
   into `feat/rust-rewrite`, post adversarial correctness reviews, fix/repeat,
   merge only on green CI, then cut a fresh follow-up branch.

## Risks and verification

- Keep once/off/context, listener mutation and nested emit behavior intact.
  An error/log listener throwing must still allow its peers and cannot recurse.
- Partial batch responses can contain both accepted IDs and rejections. Correlate
  accepted IDs to caller identities and retain complete preflight checks.
  Command return shape stays the established array of successful venue IDs;
  visible Error notices carry per-order failures, with rejection on total failure.
- Inspect current routing/signing/error contracts before changes. No full
  request/response dumps or signed diagnostics. Account failures retain last-good
  state; optional blank values must not discard economically valid positions.
- Each phase includes a plan audit, failure reproduction, focused verification,
  independent sub-agent bug review, repairs and a stable commit. Record evidence
  here and in the PR; no generated reports or profiler output are committed.

## Progress

- Fresh branch created from fetched landing. Initial worktree setup could not
  find Corepack; retry with the configured Node22 PATH passed frozen install.
- Re-verified placement stop/lost-ID path, Warning-level child failure, generic
  rejection messages and float leverage against current source. Phase1 shared
  emitter implementation and independent plan audit delegated; phase2 read-only
  audit delegated. Coordination requests sent to the two named lanes.
- Phase1 PASS: shared exchange emitter isolates synchronous listener exceptions,
  logs venue/event and original cause to console, and preserves inherited
  once/off/context plus snapshot/reentrant behavior. Seven regressions pass;
  original TinyEmitter control reproduces five failures with two API controls
  passing. Independent implementation review passes; SDK typecheck/lint pass.
  Async listener promises retain the existing contract; console reporting does
  not generate new account error/toast events or recurse through log listeners.
- Phase2 ownership confirmed in startup lane's committed plan: codex-u1pmrg
  owns known-perpetual blank ticker/startup and blank optional position numbers.
  This lane owns malformed SBE drop; its independent review passes, retaining
  per-book sequence-gap recovery and silence watchdogs. Integration/tests pending.
- Phase2 SBE PASS: 33 public tests pass. Restoring the original recovery call
  reproduces the new book-preservation failure; fixed source restored afterward.
  Independent review confirms pure decode before application, intact per-book
  gap recovery, continued valid frames, and malformed traffic cannot hide silence.
  Blank-number fixes remain coordinated with the startup lane, not duplicated.
- Phase3 PASS: all 124 venue tests (31 trading, 33 public, 23 account,
  15 history, 7 delegated, 5 signing, 10 unit), venue Clippy, format and bans
  pass. Six new failure scenarios reproduced against original trading behavior
  with 24 existing controls passing. Independent review found/fixed optional
  message-type rejection masking; its new no-state-commit test passes. All
  independent placements are attempted without replaying ambiguous writes;
  entry failures skip dependent children and retain other accepted IDs.
  Child failures use Error notices, and integer leverage matches confirmed state.
  Official integer schema: https://docs.paradex.trade/api/prod/account/upsert-account-margin
  Known TS placement bugs are declared in the harness without normalizing output.
  Lifecycle-only differences were not used as repair criteria. README's obsolete
  payload-signing rejection claim was also corrected against delegated tests.
- Phase4 local gates PASS: release standalone Paradex WASM is 1,231,577 bytes
  (fixed budget 1,258,291); actual WASM payload-signer tests 2/2 pass. Focused SDK
  tests 59/59 pass. Fresh generated private replays are identical: Paradex
  TS/Rust main 555.1/330.0ms, steady 197.6/120.1ms, retained 38.9/6.7MB;
  Binance main 100.8/41.1ms, steady 16.3/20.2ms, retained 13.7/1.1MB.
  Private timing is informational, not a performance admission claim. Full WASM
  Clippy passes. Public output/CPU/memory and workspace checks remain CI gates.
  No local public timing run or CPU-admission wait was performed.
