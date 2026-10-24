# Rust live gate: shadow, smoke and canary

Sam's current rollout mechanism is the [per-user hard-cut bundle switch](RUST_BUNDLE_ROLLOUT.md):
frozen TS app/server versus new Rust app/server for every held venue. Its cohort,
shared-state and rollback rules supersede older per-account/default-switch
recommendations below; the evidence and reconciliation requirements still apply.
This is preparation only, with no deployment or flag change.

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
  <3 active symbols> --out sdk-rs/harness/out/live/<v>-<utc-stamp>.jsonl`.
  **Always pass `--out`.** Without it the recorder writes
  `sdk-rs/harness/tapes/<v>.jsonl`, the committed fixture directory, and
  overwrites a tracked fixture. Capture once in quiet hours and once in a
  volatile hour, so listing churn, funding rollover and book bursts are covered.
- Gate with `corepack yarn rust:gate --venue <v> --tape <tape> --books <same>`.
  For public tapes the gate also enforces its CPU and memory objectives (a
  required improvement), with no flag to disable them, so a live tape can fail
  on timing alone. Only correctness failures count for this gate; committed
  fixtures stay the performance baseline. A timing-only miss is recorded and
  does not block. Implementation phase 1 adds a correctness-only mode so the
  exit code reflects this.
- Pass criteria:
  - zero divergences beyond the venue's declared ones;
  - known failures in `known-gate-failures.json` are keyed by venue plus
    `tapeSha256`, and `rust:gate:verify` reads only the CI ledger, so no fresh
    live tape matches an entry. Compare the live divergence paths with the
    diagnostic keys declared for that venue's fixture: a path outside that
    family is new. Phase 1 automates this match.
  - A new divergence is triaged into a Rust fix or a declared, reviewed TS fix
    before the venue may proceed.
- Live public tapes contain no secrets, but they are large. Keep them outside
  git (`sdk-rs/harness/out/` or local storage). Commit one only when it replaces
  a fixture, trimmed with `rust:trim-tape`. The recorder applies
  `tape-fields.mjs` at capture time: a static allowlist of field names, plus
  removal of `set-cookie` response headers. Check that before any commit.

## 2. Private shadow (read-only, credentialed)

Gap: no read-only private capture exists yet. `record.mjs` forces
`publicDataOnly: true`, and `live-smoke.mjs` places, amends and cancels.

Build `rust:shadow-private` by reusing the live-smoke profile and recorder:

- **Credentials and endpoints:** environment-only credentials through
  `liveSmokeProfile(name, mode, env)`, the reviewed per-venue environment
  profiles with their endpoint fences and redirect refusal, and the live-smoke
  recorder's credential, auth and signature redaction (`live-smoke-tape.mjs`)
  with 0600 tape files.
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
- **Venue probes to schedule with private shadow:**
  - Kraken Futures `history/v3/account-log`: find a cursor that reaches rows
    older than one full 500-row page. Only `since` is known to work (TS
    measurement, 2026-08-14), so until then income on heavy accounts returns the
    newest page with a `history.incomplete` warning. Candidates: `from`/`to`, an
    id cursor, or the v2 route. Read-only.

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

## 3a. Owner-run venue probes

These are single questions only a live account can answer. They are not part of
the generic shadow or smoke runs. Each one is **owner-run**: Sam runs it, or an
operator he delegates, on that person's own account, with environment-only
credentials, read-only, and no artifact committed. The result goes back to the
named code as a reviewed change, or as a documented limitation.

- **Kraken futures income/ledger paging past 500 rows** (from claude-cbzmw6's
  audit note).
  - **Current behaviour:** `sdk-rs/venues/kraken/src/history.rs` reads
    `/api/history/v3/account-log`. Only the `since` cursor is proven to page;
    `before` and ascending sorts return 0 rows. Past 500 rows, Rust matches TS:
    it returns the newest 500 with a truncation warning, never a silently cut
    history.
  - **Probe:** take an account with more than 500 ledger rows in one window and
    request the same window several ways:
    - `since` with `sort=asc`;
    - `since` with `sort=desc`;
    - `before` alone;
    - `before` with `since`;
    - `count` at and above 500.

    For each, record the row counts, the boundary timestamps, and whether rows
    repeat or are skipped at page edges.
  - **Pass condition:** a cursor that walks past 500 rows without gaps or
    duplicates, as checked against the account's own ledger total in the Kraken
    UI. If one exists, the Rust income pager adopts it with a regression test
    built from the redacted shapes of the measured responses. If none exists,
    the truncation is documented as a venue limit.

## 4. Canary criteria

A canary routes real accounts to Rust through the per-account
`ExchangeOptions.engine: 'rust'` override (`runsOnRustEngine`). It does not
change registry defaults. The SDK honours the override, but no tealstreet-next
caller sets it today. A canary on web or exchange-server therefore first needs:
- blocker B1 in `RUST_CUTOVER_CALLERS.md`: engine delivery, since no
  tealstreet-next build runs the Rust/wasm step yet;
- a caller change that sets the override for the cohort.

CLI and MCP can pass the override directly. Entry requirements per venue and
product:

1. Replay gates are green in CI, and public shadow (1) passes on two fresh live
   tapes.
2. Private shadow (2) passes on every applicable account shape, and the smoke
   checklist (3) passes, on the account types the canary will use.
3. Cutover-readiness consumer checks pass for the alias. No open blocker
   (B1–B8) or RT-tagged behaviour row in `RUST_CUTOVER_CALLERS.md` affects the
   canary's surface (web, exchange-server, CLI or MCP).
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
  start constructs the TS venue. That works only where the TS class still
  exists. Store state is rebuilt from the venue on start, and no
  engine-specific state is persisted.
- **Names that are already Rust by default with their TS class removed**
  (coinbase, coinbasespot, the six Orderly brokers and txflow) have no TS
  fallback. Rollback there means a fix-forward, or a revert to an earlier
  release, through the normal lane. Hosted keyless signing keeps its retained
  TS Coinbase/Orderly path (#1505).
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

1. **Live tooling.**
   - `rust:shadow-private`, the read-only runner. Scripted-transport tests cover
     the write refusal, endpoint fences, redaction, the bounded window and
     replay through `gate.mjs` private mode.
   - A correctness-only `gate.mjs` mode for live public tapes.
   - Matching live divergences against the venue's declared diagnostic-key
     families.

   Independent review, then merge.
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
