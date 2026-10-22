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
observations; delete their venue-local offsets. The service has two explicit views:
`Cx.now` remains host scheduling time for timers, deadlines, pauses and frame/book
timestamps; calibrated venue protocol time supplies outbound signing timestamps,
recvWindow/expiry and time-based nonces. Replace the correction with server time
minus raw host observation time, never accumulate it or share it across engines.
A fresh observation must affect a resend or queue drain in that same callback.
The signer captures protocol time at dispatch while admission, preparation age
and timeout comparisons retain host time. The agreed proposed accessors are
`Cx.protocol_now()` and `Cx.observe_protocol_time(server_ms)`; final implementation
follows the actual signer-core API. Regress unchanged timer/output domains and
exact tape-clock signatures. PR1 preserves existing calibration and adds none.

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
The Backpack WASM integration fixture uses an explicit clock distinct from SDK
time and checks exact REST and WS signing timestamps; the earlier fixture
expected the retired capability override and failed in CI after its removal.

Architecture gate: this is not a per-venue fix of a cross-cutting concern. One
shared production provider and instance accessor replace clock selection;
venue edits only delete obsolete opt-ins. Native `SystemTime` and explicit
server/Workers/replay clocks remain host responsibilities. Capability bit14 is
reserved and ignored by new readers; this field retirement needs paired SDK
and native artifacts. No signature/canonicalization or replay policy changes.

## PR2 execution plan after actual signer core

Base: actual #1714 `ad0e6fa1e42df883b8fe947e6b4c04eda0da2f7d`.
PR1 actually merged as #1709 `6f815755c37de240d1a87bfec6edc1e4d4c8b614`.
Owner authorized this shared calibration work after #1714; family signer
migrations separately wait for the signer prerequisites PR.

1. Add a small per-engine protocol clock service with offset/revision. Finite
   observations replace `server_ms - raw_host_ms`; equal offsets are idempotent.
   Reject invalid observations without changing offset or revision. Expose
   `Cx.protocol_now()` and `Cx.observe_protocol_time(server_ms)` while `Cx.now`
   remains raw host time. Do not put calibration in the global SDK provider.
2. Capture raw preparation time and calibration revision in pending signing
   metadata. Feed only the protocol view to canonical preparation. Both async
   completion and materialized outbound release check revision, even if raw time
   did not move. Raw timeout wins; otherwise retire stale success or error through
   the existing new-attempt retry path, preserving immutable unsigned input,
   original FIFO position and original raw deadline. Never clear preparation
   state while reusing an attempt identity that can still receive old replies.
3. Replace Bybit's `clock_offset_ms` and Aster's request `offset_ms` with the shared
   observation API. All existing outbound REST, opposite-host probes and WS auth
   use protocol time. Aster's existing monotonic nonce service keeps its namespace
   and receives the protocol view; no nonce reset or new allocation policy.
   Observation affects same-callback resend/queued drain immediately. Preserve
   existing validation and one-resend eligibility; no new write retry policy.
4. Regress independent engines, offset replacement/idempotence, nonfinite input,
   same-time calibration during async signing, late old success/error, already
   materialized outbound invalidation, unchanged raw deadlines, timeout precedence,
   same-callback venue resends, and unchanged timers/frame/book timestamps. Keep
   current Bybit/Aster native signature and nonce assertions.
5. Coordinate `engine.rs`/`signing_dispatch.rs` regions with codex-uufub4's
   prerequisite work (injected signing entropy, stamp receipts and signed upgrades).
   Do not redesign `SignStep` independently. Independent correctness/adversarial
   architecture review, focused native Bybit/Aster/Binance/engine checks with four
   jobs, fmt/Clippy, current-head CI and complete authenticated replay proof precede
   separate guard/merge/actual-pin reporting. No fixture, comparator or budget edits.

Domain contract: raw host time owns admission, staged/prepared age, pauses,
network deadlines, timers and emitted market timestamps. Calibrated protocol time
owns absolute outbound signing stamps, expiry and time-derived nonces; durations
such as recvWindow/freshness are not shifted. A changed revision invalidates both
prepared and queued signed output, never an already-transmitted request.

## PR2 implementation boundary

`Io` now owns one `ProtocolClock`; `Cx::observe_protocol_time` replaces its
correction and `Cx::protocol_now` reads it. Same-offset observations are accepted
without advancing the revision. Nonfinite observations, arithmetic overflow and
revision exhaustion leave the prior correction intact. Separate engines cannot
calibrate each other.

The shared signer captures the revision alongside raw preparation time and
passes calibrated UTC only to `SignStep::prepare`. A changed revision retires
both successful and failed pending results through the existing new-attempt
path. A completed signed action is checked again at actual release and wiped
before repreparation when stale. Original unsigned input, FIFO sequence and raw
deadline survive; raw timeout takes precedence. Already transmitted requests
are outside this path and are never replayed because of calibration.

Bybit REST, opposite-host probes and private WS auth consume the shared protocol
view; its existing eligible once-only resend feeds the observation before
rebuilding the request. Aster feeds validated `/time` responses before draining
V1/V3 requests and deletes its local offset. Sync cadence/age, four bounded
attempts, network deadlines, failed-sync retention and nonce namespace remain
unchanged. A new signed demand after a failed periodic sync still initiates its
own bounded sync attempt before release; the regression exercises that policy.

Focused verification covers 1,028 engine/Bybit/Aster/Binance native tests, denied
warning Clippy, formatting and bans. Added controls cover engine isolation,
replacement/idempotence/invalid observations, revision exhaustion, same-time
forward/backward calibration with stale success/error, late replies, materialized
outbound renewal, FIFO, repeated calibration without deadline extension, raw
frame/timer domains and same-callback venue resend/drain. Exact-head CI and
complete authenticated replay proof remain merge requirements. Architecture
review confirms one engine service and shared dispatch fences, with no new
venue-local timing policy or SDK/NX API change.

Fault controls: disabling common completion/release revision checks makes three
new signing regressions fail; feeding raw time to canonical preparation makes
the protocol timestamp regression fail. Restoring the implementation passes all
25 focused signing tests. The full 1,028-test native run is green without these
faults. No replay allowance, fixture or comparison policy is changed.
