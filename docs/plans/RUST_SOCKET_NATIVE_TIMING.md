# Opt-in native socket timing modes

Owner: codex-9czdjh. Required managed-sockets reviewer: claude-55ao7t.
Overseer assigns one shared engine PR for the three gaps identified against
6fb44235f3e. Kraken venue plan #2172 landed at 74db4cd0768; its venue originals are
a separate consumer checkpoint. The local originals checkpoint adds an inert opt-in API surface and
real engine controls; corrected runtime behavior and venue adoption follow review.

## Contract and linkage

Preserve all current adopter wire, scheduling, expiry and retirement behavior.
The new modes are opt-in; no existing venue enables them in this engine PR.
Kraken needs all three for native parity, not a new liveness or retry policy.
No venue-local Supervisor/watchdog, weaker admission, clock reconstruction,
signer change, HTTP/Session metadata or nonce authority belongs here.

Proposed nondefault engine feature: socket-native-timing, implying
managed-sockets. New variant handling, state and methods are compiled only with
that feature. Existing feature graphs and harness/family declarations remain
unchanged. This keeps the new implementation out of non-adopter WASM binaries;
paired builds must still prove zero growth, rather than treating the gate as
proof. With the feature enabled, old policies retain exact behavior too.

55ao7t plan/API direction CLEAR at c7d95e91877. The additive registration
carrier is approved instead of a required Config or FeedPolicy field:

```text
StableAnchor = Ready (default) | Open
NativeTiming { initial_payload_timeout_ms: Option<f64>, stable_anchor }
Cx::register_socket_feed_with_native_timing(policy, timing) -> Result<FeedId, SocketError>
Cx::complete_socket_initial_payload() -> Result<(), SocketError>
```

The stable_anchor setting reaches Supervisor only at registration (constructor
argument/set-once), outside Config. Default Ready reproduces the old registration
paths bit-for-bit. Existing full Config literals in Bybit, Phemex and engine tests
remain untouched, including with feature unification. The timeout is retained in
Driver Runtime beside snapshot_timeout, validated in the same atomic preflight.
Old registration and the Phemex snapshot entry remain unchanged.

## Declared receive refresh

Add Control::Traffic and TrafficPolicy::Declared. Traffic is delivered exactly
like Other, so valid native data still reaches its reducer. Declared refresh is
exactly Traffic, Pong, Auth(Accepted) and Auth(Progress). Other, malformed and
rejected auth evidence do not refresh. Existing policies must ignore the new
decision and retain their current rules. Declared requires Native controls;
Declared with ControlPolicy::None refuses InvalidPolicy before mutation.

Kraken's future pure codec supplies Traffic for any successfully decoded native
Ws envelope, including permissive defaults/empty envelopes. It supplies Other
for malformed/wrong-type envelopes; valid auth evidence retains its auth effect.
No generic JSON parser, free-form refresh callback or venue receive timestamp.
All existing current-epoch, raw-time and expiry checks run before decoding or
refresh. Progress renews only the ruled auth stage; Declared additionally records
its genuine receive time with one received(now) in finish_control, without a
second auth renewal or readiness grant; the Ready-only failure reset remains.

## Independent OPEN initial-payload bound

A new optional finite-positive timeout is independent of Config.auth_timeout_ms
and the existing CONNECT-origin Phemex snapshot. The Kraken consumer will select
180000 ms. Registration validates the complete policy/options and capacity before
publishing a feed; combining incompatible snapshot modes refuses. Exact permitted
policy combinations are an API-review gate; the initial consumer is Private,
Required auth, Native controls and Reconnect recovery.

Arm one checked absolute deadline on the genuine current Opened transition,
never on CONNECT, ACK, Progress, receive refresh or a signing attempt. Include
it in the existing Supervisor watch and synchronous expiry predicate in both
Authenticating and Ready. Expiry at equality retires through the existing typed
snapshot-timeout path, with no new timer owner or retry algorithm. Retirement
clears it before replacement; duplicate open cannot re-arm it. Allocate/re-arm
through existing transactional/fatal resource rails, with no partial grant.

The new completion method uses only the engine's current native text-message
proof. It accepts an armed current epoch in Authenticating or Ready, before or
after final ACK, strictly before all applicable expiry. It clears only this
initial-payload bound once; duplicates are inert/typed stale. It never calls
authenticated(), changes auth/Ready, alters stable/receive time, completes the
old snapshot mode, or releases interests. No caller epoch/time, cached proof,
lifecycle callback, HTTP/timer/command/signing or binary completion.

The future Kraken codec calls it only after validating all three native initial
payloads (fills, open_orders, open_positions); native valid empty snapshots count.
Control-only ACKs/heartbeats and malformed rows never complete it. The critical
order is all three empty payloads BEFORE final ACK, final ACK grants Ready, then
quiet native pongs stay healthy beyond OPEN+180s. This requires the new completion
entry: today's authenticated-only complete_socket_snapshot cannot express it.

Phemex's existing authenticated CONNECT-origin completion remains byte- and
behavior-identical, including its refusal before login and exact message custody.

## Stable anchor

Keep auth deadline origin separate from stability origin. Connecting/opened
initialize the stable origin; Ready default reproduces today's authenticated
transition reset. Open selects the genuine OPEN origin, which neither Progress
nor final ACK can move. Non-auth feeds keep their native open-based behavior.
received() retains the existing Ready-only reset condition and stable_ms value,
using the selected origin. No reset merely from open/authentication, no new
backoff counter, and no reset from a synthetic timestamp. Invalid/overflowing
time cannot create permission or move an existing bound.

## Originals and source gates

After 55ao7t plan/API direction CLEAR, add real-engine/driver originals before
runtime changes. New behavior needs assertion REDs against an inert surface or
native baseline; existing-adopter controls must be GREEN on the baseline. No
compiler error, absent API, fake result or ignored test counts as an original.

- Declared Traffic reaches the exact native reducer and refreshes; Other does
  not. Valid challenge/partial ACK/final ACK/permissive empty envelope refresh;
  malformed/wrong-type input cannot rescue expiry. Old policy traffic unchanged.
- OPEN+180s differs from CONNECT+180s and from each 15s auth stage. Delayed open,
  Progress and final ACK do not move it; completion before final ACK grants no
  auth/Ready. Missing/invalid/duplicate completion and equality are fenced.
- Real current text completion, expired/retired/foreign/binary/HTTP/timer/command
  and lifecycle scopes; duplicate and allocation refusal preserve truthful state.
- Phemex's old snapshot path stays authenticated/CONNECT-coupled and cannot be
  discharged by the new method or vice versa. Neither mode borrows cached proof.
- Flapping and delayed auth prove Ready default versus Open selection at the
  same 30s threshold, genuine traffic-only reset, Progress and duplicate ACKs.
- Existing OKX, Phemex, Backpack and WEEX originals pin complete wire/action,
  timer, interest, snapshot, health, resource and retirement behavior. Preserve
  their old assertion bodies/clock fixtures. Mode-enabled and disabled runs must
  agree for every adopter that does not opt in.
- NaN/infinity, overflow, resource exhaustion, duplicate/stale events, dispose,
  queued sends and sibling isolation retain the shared fatal/custody rails.

Source is restricted to engine socket state/driver/ingress, engine feature and
exports, and controls/docs. No venue dependency or inventory edits. Audit all venue
Control matches for exhaustive matching under the enabled feature, and record
that result in the PR; no speculative non_exhaustive change. Add explicit OFF/ON
rows to existing feature-mode CI with strict Clippy for the enabled mode. Merge
actual #2161 when landed; the shared engine_socket.rs delivery-match adjacency
must preserve its handoff capability work, not consume its unlanded source. Required
review is 55ao7t; uufub4 can review custody where exact completion/epoch semantics
need it. One shared PR carries the plan, originals and corrected source.

## Qualification and size gate

Merge actual origin/feat/rust-rewrite before measuring. Pair bare tip and composed
head with identical current tools/profiles for Bybit plain and managed-sockets
linked, Bitget, OKX and combined, with the new feature OFF; each
non-adopter must show ZERO growth. Record features and hashes, not just byte
counts. Do not hide growth by changing budgets, profiles, feature declarations
or the measurement command. Compare the actual shipping feature graph; separately
run enabled-mode existing-adopter behavior/compile controls. Any proposal to
change that linkage contract returns to the owner/overseer before source.

CI is the family-size record; one local composed combined is mandatory before
READY. Delete each own scratch target after measurement, retain sources/logs and
tables, and run builds serially with JOBS4. Scoped strict checks, feature OFF/ON
and existing-adopter controls, full bans, source review and exact-head green CI
precede READY to 8x5tm1. Auditor exact-tree GO precedes guarded owner SDK merge.
No Kraken activation, namespace takeover, hosted adapter or deploy is claimed.

## Local compiling originals checkpoint

On composition 26c48fbd6e4 (actual landing 74db4cd0768 included), 25 new real-engine
controls execute: 17 intended assertion REDs and 8 working GREENs, zero ignored.
The inert registration ignores NativeTiming, initial completion returns Stale,
and Declared currently uses PongOnly while Traffic is not delivered. These are
original surfaces, not the timing implementation or permission to enable a venue.
Four initial fixture compilation errors were corrected before this receipt; no
compiler failure is counted as a behavioral RED.

The harness retains actual SetTimer/ClearTimer handles, delivers due timers in
order and uses real Open/Message/Command/HTTP/Binary/lifecycle callbacks. Each
invalid timeout has its own test, so NaN and both infinities actually execute.
The scope control reaches real forbidden callbacks before its first success RED.
Earlier REDs gate later completion/duplicate/quiet-traffic suffixes; they are not
claimed as executed success evidence. The shared completion fixture supplies the
venue text boundary; Kraken's all-three-native-payload decode proof remains a
separate venue original after #2172.

Strict enabled-mode library/test Clippy passes with warnings denied. The existing
engine socket controls execute 138 PASS, zero failed/ignored, with the new feature
enabled and only this intentionally RED module excluded. Existing venue assertion
bodies are unchanged; their enabled-mode runs and feature-OFF parity remain
qualification gates. No feature-mode CI row is added until source makes the new
originals GREEN. No build profile, harness, venue dependency or inventory changes.

## Corrected source checkpoint

After 55ao7t ORIGINALS/API CLEAR at 04bc5bcf683, actual #2162 landing 0a54e37b1e8
is plain-merged. The old Bitget CPU exception does not apply to qualification.
The original enabled-mode existing-adopter targets execute 94 PASS (37 Backpack,
21 OKX, 16 Phemex private, 9 Phemex public, 11 WEEX), with unchanged test bodies.
The 138 existing engine socket controls also pass WITH socket-native-timing on.
Both original review-build targets are removed after completion.

The corrected source makes all 25 new controls GREEN. The Progress control adds
an exact auth expiry check: after Progress at 55000 with a 100000 ms auth stage,
Pongs keep receive live, 154999 remains current and 155000 retires for Auth.
No pre-existing assertion is changed. This executes the previously RED-gated
completion/duplicate/quiet-traffic suffixes too; it still does not substitute for
Kraken's separate all-three-native-payload codec proof.

Driver registration validates finite-positive timeout and the closed Private /
Required / Native / Reconnect policy before its existing registration publishes.
Snapshot and native timing use separate registration entries; there is no API
that installs both timeout modes on one feed. Native timing contributes its delay
to the existing arithmetic representability preflight. The stable anchor is set
only on a newly constructed Supervisor. Existing Config/FeedPolicy shapes stay
unchanged, and the source audit finds no exhaustive Control/TrafficPolicy match
in venue crates; no non_exhaustive annotation is needed.

OPEN arms the initial deadline before the existing watch is rearmed. Completion
consumes only current text scope and clears only that deadline. It allocates no
resource and leaves an earlier watch harmlessly armed to recheck the surviving
bounds. It never calls authenticated or changes Ready/receive/stability state.
Declared refresh is computed from closed native control evidence and uses the
single existing received(now) call in finish_control for auth transitions.
Ready remains the exact old stable-origin calculation; Open uses the dedicated
OPEN origin. Explicit feature-mode OFF/ON rows include enabled strict Clippy.

Source review, refreshed existing-adopter controls, feature-OFF pairs, scoped
strict/fmt/bans and exact CI remain gates; no size or publication claim yet.

55ao7t SOURCE CLEAR binds a1d81fc8e646. Initial-payload expiry uses the existing
RetireReason::Timeout(Timeout::Auth), even after Ready; it is not AuthFailure
evidence or a definitive auth refusal. The Kraken consumer plan records that
distinction so no credential latch or write resend is inferred from retirement.
