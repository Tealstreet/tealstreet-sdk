# Release-paced serial Plans (`serial-pacing`)

Owner: claude-dazzy3 (overseer assignment). Reviewer: uufub4 (nonce and release
interaction). Base: `feat/rust-rewrite` `b0ee0a9a4d5`. **Plan only; no source.**
Consumer: Hyperliquid trading (`RUST_HYPERLIQUID_TRADING_CONTEXT.md`, the
"Actual-release pacing" gate). The required originals come from uufub4's review
of plan `f5398bc7f9f`.

## Ruling being implemented

Native Hyperliquid spaces writes 334 ms from the previous write's *queue* time
(`trading.rs:582`, `write_at` set at `:701`). The overseer ruled the route:
**pace from the previous call's final release**, reusing the registered
`Pacing`/`SendPacing` floor that `commit_claim` already advances.

This is a ruled behaviour change, not native parity:
- It is never faster than native. The release time is never earlier than the
  queue time, so `release + 334 >= queue + 334`.
- It can add latency when host phases (Prepare, sign, Bind, Claim) are delayed.
  Example: queue 0, Claim 1000, DATA 1001. Native sends the next write at 1001;
  release-paced sends it at 1334.

CALLERS must record both points. There is no venue-local timer or shadow floor,
and no queue-time ledger.

## What already exists (no change)

- `Context` carries `pacing: Option<Pacing>`. Plan calls take it from
  `PlanPolicy.pacing`, which `admit_serial_plan` already passes through.
- `prepare_claim` checks `SendPacing::check(context.pacing, raw_ms)` before any
  admission or nonce floor commit:
  - it returns `WaitUntil(at)` with no release;
  - it returns `TimedOut` if `at >= deadline`.
- `commit_claim` alone runs `SendPacing::released(pacing, raw_ms)`. A refused,
  stale, duplicate or WaitUntil claim spends no floor.
- Floors are per group and engine-wide, so they span serial roots: the next
  command's first call waits on the previous command's last release (native
  FIFO handoff).

## Change, behind a non-default feature

1. **Feature.** `serial-pacing = ["serial-plans"]` in `engine/Cargo.toml`.
   No family opts in here; Hyperliquid opts in at its own adoption PR.
2. **Admission, `admit_serial_plan`.**
   - Feature OFF: byte-for-byte as today. A nonzero literal gap is `Invalid`,
     and `validate_zero` rejects nonzero registered floors.
   - Feature ON: a policy pacing that names a registered group or set is
     accepted with its registered floors.
   - The policy's literal `minimum_gap_ms` must stay `0`. The registry is the
     single authority, so a call cannot widen or invent a floor.
   - An unregistered group stays `Invalid`, through the existing
     `resolve_pacing`.
   - This is the capability gate: only a venue that both enables the feature and
     registers the group in `http_pacing()` gets nonzero serial pacing.
3. **Nothing else moves.**
   - Continuations (`continue_serial_plan` and `_after`) keep their eligibility
     semantics. The floor binds at Claim, not at admission or continuation.
   - The per-call bound (`now + timeout_ms`, 20 s for Hyperliquid) is never
     renewed by a pacing wait or a re-prepare. A floor at or past the call
     deadline is `TimedOut` before release, i.e. NotSent.
   - The serial root stays unbounded.

Expected source: about 10 lines in `request_context_serial_plan.rs` under
`#[cfg(feature = "serial-pacing")]`, plus the feature line.

## Originals (RED on the base unless marked GREEN)

These are engine tests in a new `serial_pacing_tests.rs`, compiled only with
the feature. Today admission refuses, so each is RED at admission. All of them
use a registered 334 ms group.

1. **Early, equality, late:**
   - a Claim at prev+333 is `WaitUntil(prev+334)` with no release;
   - a Claim at prev+334 is Released;
   - a late Claim releases at once;
   - the floor then re-anchors at the actual release.
2. **Delayed host phases:**
   - delayed Prepare, Bind or Claim, or slow or fast DATA, anchor the next floor
     at the real final release;
   - the queue0/Claim1000/DATA1001 example gives next 1334.
3. **Continuation and FIFO handoff:**
   - same-root `continue_serial_plan` and `_after` both wait on the floor;
   - a second command's first call waits on the first command's last release;
   - `done_empty_plan` (no calls) moves no floor.
4. **No credit spent:** none of these advance the floor or consume a nonce:
   - a preparation or provider refusal;
   - stale nonce repair (re-prepare);
   - a duplicate claim;
   - a non-finite or backward clock at Claim.
5. **Bound:**
   - a floor past the 20 s call deadline is `TimedOut`, NotSent, with no release;
   - a WaitUntil never extends the call deadline;
   - the root remains unbounded.
6. **Nonce interaction (with `dispatch-nonce`):**
   - a WaitUntil claim reserves and commits no nonce floor;
   - the released call's nonce is committed only under the final commit.
7. **GREEN controls with the feature on:**
   - a zero-gap serial Plan is unchanged;
   - an unregistered group is `Invalid`;
   - a nonzero literal policy gap is `Invalid`.
8. **GREEN control with the feature off:** nonzero serial pacing is still
   `Invalid`, unchanged (an existing-behaviour pin).

Hyperliquid's own native timing originals (builder 8 s prerequisites, venue
queue) belong to its adoption PR, not here.

## Sizes and checks

- **Zero cost when off.** Paired Bybit, Bitget and combined on the composed
  tip, A = tip and B = this change with the feature off, must be identical.
  The feature-on build is measured only as an engine test build. No family
  enables it here.
- **Checks:**
  - engine tests in default, `--features serial-pacing` and
    `serial-pacing,dispatch-nonce`;
  - the 26-mode feature script, with this feature added as a mode if the script
    enumerates engine features;
  - clippy in three modes, wasm32 clippy, bans.
- **CI:** one PR run on a frozen head.
