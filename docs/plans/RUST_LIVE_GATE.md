# Rust live gate: shadow, smoke and canary

How a Rust venue proves it matches TS against the live venue before any
account defaults to it. Replay gates (`rust:gate`, `RUST_GATE_HONESTY.md`)
prove parity on recorded input; [cutover readiness](RUST_CUTOVER_READINESS.md)
proves consumer contracts; [live smoke](RUST_LIVE_SMOKE.md) proves one
credentialed order lifecycle. This gate adds live evidence on current venue
data and states what has to hold before cutover, and how to roll back.

Out of scope: deploys, default changes, TS venue deletion, and committing real
credentials or account-bearing tapes. Live runs are manual and operator-run;
CI keeps replaying committed tapes only.

## Principle: capture live, compare offline

Two engines connected to a venue at the same time never see identical input.
Arrival order, snapshot timing and reconnects differ, so a live A/B diff
mostly measures timing. The gate therefore keeps the comparator that already
certifies replay:

1. **Capture.** Record one live session to a tape. Every HTTP response and
   websocket frame is recorded with its time.
2. **Dual replay.** `gate.mjs` replays that tape through the frozen TS venue and
   the Rust engine. It requires identical store output and books
   (`snapshot-compare.mjs`, `comparison-policy.mjs`). Every divergence is either
   a declared `KNOWN_TS_FIXES` entry with a reason or a failure; nothing is
   normalized away.

A live tape is new, current input for the same proof. When a live tape
diverges, the divergence is either a Rust bug or an undocumented TS
behaviour. Either one blocks cutover until it is fixed or declared.

## 1. Public shadow (no credentials)

Per venue and product (the Rust aliases in `RUST_VENUES`):

- Record with `corepack yarn rust:record --venue <v> --seconds 300 --books
  <3 active symbols>`. Capture once in quiet hours and once in a volatile hour,
  so listing churn, funding rollover and book bursts are covered.
- Gate with `corepack yarn rust:gate --venue <v> --tape <tape> --books <same>`.
  Correctness must PASS. CPU and memory results are informational for live
  tapes, because committed public fixtures stay the performance baseline.
- Pass criteria:
  - zero unexpected divergences;
  - every known failure in `known-gate-failures.json` still matches exactly
    (`rust:gate:verify`).
  - A new divergence is triaged into a Rust fix or a declared, reviewed TS fix
    before the venue may proceed.
- Live public tapes contain no secrets, but they are large. Keep them outside
  git (`sdk-rs/harness/out/` or local storage). Commit one only when it replaces
  a fixture, trimmed with `rust:trim-tape`. Tape scrubbing in
  `tape-fields.mjs` (cookies and auth headers) applies before any commit.

## 2. Private shadow (read-only, credentialed)

Gap: no read-only private capture exists yet. `record.mjs` forces
`publicDataOnly: true`, and `live-smoke.mjs` places, amends and cancels.

Build `rust:shadow-private` by reusing the live-smoke profile and recorder:

- **Credentials and endpoints:** environment-only credentials
  (`liveSmokeProfile(env)`), the reviewed per-venue environment profiles with
  their endpoint fences and redirect refusal, and the same recorder redaction
  and 0600 tape files.
- **What it does:** start private, wait for readiness, hold for a bounded window
  (default 120 s, so order, position and balance streams and one REST
  reconciliation are captured), then dispose.
- **What it must never do:** the runner may issue no venue command. The command
  path is disabled at the host, and any write attempt aborts the run with
  `write-refused`. Its test proves this with a scripted transport.
- **Accounts:** a **read-only API key** on each venue that offers one. Otherwise
  use a dedicated account whose key has trading disabled. Never use a customer
  key.
- **Comparison:** the captured tape replays through `gate.mjs` private mode,
  which already supports private tapes, and checks balance, positions, orders,
  settings and fills. A private live tape stays local: it contains account data
  and is never committed. Only a redacted summary goes on the PR: the venue,
  account shape, frame counts and the PASS/FAIL list.
- **Account shapes to cover per venue,** where the venue supports them: empty
  account; resting limit orders; an open one-way position; a hedge-mode
  position; a protective TP/SL; spot balances (spot products). Shapes are set
  up by hand in the venue UI beforehand; the runner never creates them.

## 3. Smoke checklist (credentialed, writes)

Run `corepack yarn rust:live-smoke --venue <v> [--testnet|--demo|--mainnet]`
(`RUST_LIVE_SMOKE.md`) on a dedicated empty test or demo account. `--mainnet`
needs the explicit switch and a minimum-size budget agreed with Sam. Each step
passes or the venue fails; a skipped step is never counted as passing.

- [ ] Private readiness within the timeout; account frame inspected
- [ ] Minimum-size resting limit placed once, below the bid
- [ ] Amend (native or cancel-replace); original and returned IDs both retained
- [ ] Cancel by ID, then cancel the symbol
- [ ] Leverage lowered where supported, then restored to the captured original
- [ ] A fresh engine after disposal sees the cleaned-up remote state
- [ ] The captured tape passes `rust:gate` (TS/Rust replay of the lifecycle)
- [ ] No credential, signature or signed URL in stdout, stderr or the tape
  (`tape-security.test.mjs` rules)

Venues whose profile refuses a mode, or that have no profile, stay at "no live
write proof". That status is listed per venue on the readiness table; it is not
inferred.

## 4. Canary criteria

A canary routes real accounts to Rust through the existing per-account
`ExchangeOptions.engine: 'rust'` override (`runsOnRustEngine`). It does not
change registry defaults. Entry requirements per venue and product:

1. Replay gates are green in CI, and public shadow (1) passes on two fresh live
   tapes.
2. Private shadow (2) passes on every applicable account shape, and the smoke
   checklist (3) passes, on the account types the canary will use.
3. Cutover-readiness consumer checks pass for the alias. Its row in
   `RUST_CUTOVER_CALLERS.md` has no open RT blocker for the canary's surface
   (web, exchange-server, CLI or MCP).
4. The canary cohort is internal accounts first (Sam or team), then opt-in
   users. Each stage runs at least 7 days and through at least one venue
   maintenance window or reconnect storm.

During the canary, watch these signals per account (from the store and the
host, no new telemetry dependency):

- **Divergence:** a nightly private shadow of each canary account against TS
  (section 2) has zero unexpected diffs.
- **Startup:** no `StartupFailed` except a classified credential rejection;
  `validateAccount` matches TS for the same key.
- **Commands:** no `CommandFailed` with outcome `unknown` that TS would have
  resolved, no duplicate placements, and caller client-order IDs honoured
  (`packages/safe-cex/CLIENT_ORDER_IDS.md`).
- **Liveness:** `store.socketHealth` reconnect and close counts are no worse
  than TS on the same venue, there are no `engine-dead` events, and
  `requestStats` shows no auth or rate-limit rate above TS.
- **User-visible:** no support report traced to the engine.

Exit to a default change (a separate, reviewed PR per venue): all signals clean
for the full canary, plus Sam's sign-off.

## 5. Rollback

- **Per account:** remove the `engine: 'rust'` override. The next account
  start constructs the TS venue. The TS venues stay in the tree until deletion
  prerequisites are met, so rollback never needs a build. Store state is
  rebuilt from the venue on start, and no engine-specific state is persisted.
- **Per venue default:** revert the default-change PR. Defaults are registry
  data, so a revert is one commit through the normal release lane.
- **Triggers:**
  - any duplicate or unintended order;
  - any unsigned or rejected-signature burst;
  - private-shadow divergence on balances or positions;
  - an `engine-dead` event;
  - startup failures above TS for the same keys.
  Roll back first, then diagnose from the private shadow tape.
- **Interrupted writes during rollback** keep their outcome `unknown`.
  Reconcile from the venue (`findOrderByClientOrderId`) before any retry, and
  never replay automatically.

## Implementation phases

1. **`rust:shadow-private`:** the read-only runner. Scripted-transport tests
   cover the write refusal, endpoint fences, redaction, the bounded window and
   replay through `gate.mjs` private mode. Independent review, then merge.
2. **Live-evidence table:** a per-venue row in `RUST_CUTOVER_READINESS.md`:
   - public shadow date and result;
   - private shapes covered;
   - smoke mode;
   - canary stage.
   Rows are filled only from actual operator runs; this file records no claim
   of a run that has not happened.
3. **Operator runs:** Sam or a delegated operator runs them with environment
   credentials. Results go on the venue's PR or tracking issue as redacted
   summaries. No real-key or account-bearing artifact is committed.
