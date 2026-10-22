# Engine clock standardisation

Owner: codex-5c57jl; base `e42a32a8e945`; owner directive 2026-10-22.
Clock policy is cross-cutting. Fix the shared engine clock ingress rather than
adding another venue opt-in. Signing core and Binance belong to codex-uufub4;
its dispatch preparation continues to consume `Cx.now: f64` epoch milliseconds.

## Contract and phases

1. PR1: audit clock ingress and tape-clock injection. The default SDK transport
   waits for shared clock startup and dynamically reads synchronized time before
   startup and on every later event. Concurrent starts join an in-flight initial
   synchronization. Explicit host/replay clock providers remain
   authoritative, dynamic and common to commands, HTTP/WS completions, timers,
   nonces, expiry and signer dispatch; never overwrite an injected tape clock.
   All venues consume the same engine event-time contract without name lists.
2. Retire `host_clock` / `hostClock` as a per-venue policy and remove its native,
   SDK and fixture opt-ins. Preserve binary flag positions: reserve former bit14;
   never shift later fields. Update capability/frame tests and nearby specs.
   Existing Bybit/Aster server calibration is deferred to PR2 as directed below.
3. Regress formerly unsynchronized venue families and former opt-ins through
   startup, commands, WS authentication, HTTP/sign callbacks and timer deadlines;
   prove resynchronization is read dynamically. Regress injected tape clocks even
   when synchronized wall time differs, plus append-only capability ABI. Prove
   failures before fixes and keep all earlier assertions/financial identity proof.
4. Independent correctness/adversarial review must explicitly answer whether this
   is a per-venue fix of a cross-cutting concern (answer: no, one shared ingress).
   Focused native/host/SDK tests, typecheck/lint/Clippy/fmt/bans locally (four Cargo
   jobs); CI owns builds, all family replay/signature parity and fixed budgets.
   No fixture, comparator, allowance, timing or budget relaxation. Integrate any
   relevant actual core/landing change, fresh CI, separate exact revert guard,
   merge green reviewed head, verify actual tree/parents and report the pin.

## Coordination and limits

Owner decision: PR2 follows the actual signer-core merge. Add one engine
clock-calibration service fed by Bybit 10002 server observations and Aster `/time`
observations; delete their venue-local offsets. Signing reads the engine clock at
send. Explicitly document and replay-test the resulting timer/output-domain
change. PR1 preserves these existing protocol paths and adds no calibration.

No change to the `Cx.now` accessor or primitive Signer clock stamping: signer
primitives sign exact bytes and do not sample independent wall time. Coordinate
shared capabilities/SDK/host files with codex-uufub4; engine request/dispatch and
pending-sign logic remain in its core. Ask for a merge window if clock work needs
engine dispatch changes. No venue signer migration until actual signer core.
No TS deletion, default engine flip, publishing or NX edits.

## PR1 implementation evidence

The shared SDK clock accessor also supplies local schedule-deadline validation
and default Bybit/WOO journal bounds. Type/finite validation stays before JSON;
worker calls let the authoritative worker validate the deadline. Shared clock
initialization is inside the existing disposal-aware loader wait, so disposal
settles one account without cancelling synchronization needed by its peers.

Regressions cover all 49 registered names, dynamic forward/backward time,
commands/frames/WS/timers, streamed HTTP callbacks, concurrent initialization,
disposal, legacy capability records and divergent explicit tape clocks. The
old ingress failed 40 of the first 52 controls; fault-disabling the initialization
barrier and the disposal regression independently reproduced their failures.
Explicit-clock deadline and both journal-bound controls also failed before fix.
Final focused checks: 1,344 native tests, 428 SDK/routing/startup tests, the default
combined-host capability fixture, 76 host/wire/tape controls, typecheck/lint,
Clippy/fmt/bans. Full replay/signature/build proof belongs to exact-head CI.

Architecture gate: this is not a per-venue fix of a cross-cutting concern. One
shared production provider and instance accessor replace clock selection;
venue edits only delete obsolete opt-ins. Native `SystemTime` and explicit
server/Workers/replay clocks remain host responsibilities. Capability bit14 is
reserved and ignored by new readers; this field retirement needs paired SDK
and native artifacts. No signature/canonicalization or replay policy changes.
