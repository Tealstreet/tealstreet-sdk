# Deterministic Aster replay clock regression

The existing delayed-startup regression failed in CI run39651803685,
job119402081578 during Hyperliquid Spot #1653. Aster's current private replay
rewinds Date.now on readiness while dayjs's no-argument Date keeps wall time;
a 1500ms real delay and firstcommand at600ms leave roughly100ms future-window
headroom. The authorized single job rerun119409243794 passed. Replace the test's
wall-time dependency, preserving real sync/signing/transport/request validation.

Change only sdk-rs/harness/asterdex-clock.test.mjs and this plan.
Exercise two isolated child profiles with controlled signing/replay time:
- Continuous original-regression config (test-only virtualPrivateClock=false).
  Inject21000ms module setup and1500ms after realclock sync; supply deterministic
  tape pacing without overriding loader/deadline time. Assert actual raw signed
  timestamp and validator oracle equal fixtureepoch+1500+frametime.
- Current virtual config, real ReplayClock frozenstartup/start/timer behavior.
  Make no-arg Date follow tape time; inject1500ms startup and assert rewind,
  exactcommandtimestamp/receipt, zeroinvalid/unserved/unusedwrites.

Use a testloader scoped performance binding in replay.mjs; global freezing
before TS imports was rejected in prototype because it stalled module loading.
Install controlled Date only at actual clock.start; preserve explicit Date
arguments, parse/UTC/static behavior. Capture real validator inputs/results
before request canonicalization. No signer or getCurrentTimeMs stub.

Keep timestamp/recvWindow/signature checks unchanged. Fail closed on loader
replacement count and profile override. Negative controls must fail through the
real timestamp validator: remove TS post-importclockStart reset, or rebase the
continuous signing clock at readiness. Retain all original receipt assertions.

Inspect live offset path and record evidence: serverAtSync+(wallNow-clientAtSync)
advances with server time after startup; narrowmargin arises from test clocks.
Report a live defect separately only if demonstrated; no production edits here.

Independent plan/implementation review; focusedclocktest and fullharness tests;
freshfullhead CI with ordinary onepairdefault; freshseparateguard, mergecommit,
actualparent/files audit and report. Then resume Kraken and remaining sweep.
