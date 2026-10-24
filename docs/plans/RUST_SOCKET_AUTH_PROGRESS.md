# Managed socket authentication progress

## Owner, base and scope

Owner: codex-5c57jl. Base: actual `a10974b2f22a2be01b0cc68ff78f91bd0e253539`.
This separate prerequisite implements the overseer ruling for Kraken Futures;
reviewers are uufub4 for socket custody and 9czdjh for the Kraken consumer.
Plan and executable original REDs precede runtime changes. No venue, signing
recipe, Session, HTTP, inventory, checker, host or rollout change belongs here.

Kraken whole-family adoption stays held until this prerequisite lands. Public
and private Kraken sockets must move together: a managed private feed alongside
legacy public dispatch is not an acceptable consumer composition.

## Native facts

Kraken `account_ws_open` sets its native auth deadline to OPEN receipt +15s and
sends the Primary key-only challenge request. CONNECT has its separate connection
bound and starts no challenge wait. One accepted nonempty challenge while
`waiting_challenge` is true captures the challenge, clears that flag and resets
the native deadline to challenge receipt +15s while signed feed subscriptions
and their ACKs are outstanding. Duplicate, empty, foreign or retired challenges
cannot reset it. The all-feed ACK condition ends authentication; the independent
180s missing-initial-payload watchdog remains a distinct native obligation.

This is not #1970's CONNECT-origin snapshot bound. Do not classify a challenge as
full AuthEvidence::Accepted, reset a snapshot deadline, fabricate a Session or add
a venue auth timer. The existing normal auth policy starts at OPEN; accepted
progress must re-arm only that auth clock without granting readiness.

## Proposed narrow API and custody

Add payload-free `AuthEvidence::Progress` to the existing control evidence enum.
It carries neither a chosen epoch nor a duration. Only the engine's admitted
current text ingress and native pure codec can present that evidence. The codec
must validate the native phase and nonempty challenge predicate; Kraken accepts
one challenge per epoch. The core stores no challenge/signature and does not
invent a second parser or a generic challenge protocol.

`Lifecycle::auth` returns an internal Progress transition only for the exact
current epoch, Required auth, Authenticating phase, and not-yet-authenticated
feed. Ready/authenticated, None/Observe policy, retired/foreign epochs ignore
Progress. No authenticated bit, phase, interest proposal/revision/coverage,
first-ready fact, retry counter or permission changes.

The driver handles this transition with one narrow Supervisor hook, proposed
`authentication_progress(cx)`. The hook is internal to managed sockets and only
acts in the existing Authenticating phase. It uses the configured auth timeout
and the admitted message receipt clock; finite/non-overflowing deadline checks
precede mutation. It updates the existing auth origin and re-arms the existing
watch through normal Supervisor custody. No new independent timer, clock,
allocation or public Cx deadline setter is introduced. A refused progress leaves
the original auth bound intact. Existing fatal/resource-exhaustion rails remain
in force; timer allocation must not expose partially granted auth or interests.

Ingress and control observation retain their current expiry check BEFORE the
codec, progress, receive mutation or native publication. Equality expires: a
challenge at OPEN+15s cannot revive the connection when timer ingress is delayed.
A challenge just before that boundary can renew to its receipt+15s, and equality
at the renewed bound still fences provider results and sends. Invalid/nonfinite clocks grant nothing. The hook uses the host-provided raw
receipt clock under the existing monotonic host-clock contract. No timeout, physical signing attempt or
signer freshness is renewed by this operation; send_signed retains its existing
current-epoch and final-release gates.

Progress emits no Opened/Ready/Authenticated/native refresh callback and releases
no desired subscriptions or released-interest continuation. Explicit native
challenge/subscription authentication frames may still use current-epoch
send_signed while auth is pending, just as an opening login does. Ordinary
interests stay gated until genuine all-feed auth acceptance. Consumer integration
must bind the recipe to that epoch and immutable accepted challenge, preserve
exact feed order/ACK semantics, and show late provider results inert after
retirement. That consumer/recipe work is outside this prerequisite.

Default feeds that never emit Progress retain their exact existing semantics.
#1970 snapshot registration retains its immutable connect-origin deadline;
Progress cannot extend or discharge that independent barrier. The native Kraken
180s payload watchdog is neither moved nor discharged by progress or final ACK.

## Original evidence and source gates

Use a dedicated actual Engine managed-socket target and genuine open/current
text/timer/close/retry/replacement ingress. The original fixture maps its named
accepted-challenge event to Control::Other while the API is absent; desired
assertions must fail on unmodified runtime, not on missing enum/compile/setup.
At adoption only this explicit fixture evidence mapping changes to Progress;
all contract assertions remain identifiable. No synthetic readiness or epoch is
minted to force the fixture.

Required controls:

- Delayed OPEN starts the auth window then, not at CONNECT; no challenge timeout
  before OPEN, with the existing connect bound separately preserved.
- OPEN at 0, accepted challenge at 14s: alive at 15s, auth expires at 29s without
  final ACK; no Ready or subscription release from challenge alone.
- Full final auth acceptance releases the desired private set once; duplicate
  final ACK and progress after acceptance do not produce another release.
- Malformed/empty/duplicate/ordinary text never renew the native challenge phase;
  pong/AnyText liveness is independent from auth progress.
- At both original and renewed equality boundaries, expiry wins before codec,
  callback, provider release or publication without TimerFired first.
- Real retirement/reconnect yields a different epoch; old socket challenge,
  timer and parked signing result cannot mutate or send on the replacement.
- Progress cannot extend #1970's retained snapshot barrier or mark snapshot done.
- Same existing watch custody survives early/stale timer ingress; refusal and
  invalid clock/deadline preflight do not consume or lend authority.

Before runtime: immutable plan/original checkpoint reviewed by uufub4 and
9czdjh. Before draft/freeze: scoped engine controls and relevant socket targets,
strict Clippy/fmt, unchanged default controls, normal bans/ratchets, and exact
source/API review. CARGO_BUILD_JOBS=4, one local Cargo process; no full workspace
or all-feature local sweep. Push only a freeze candidate; one PR-triggered Rust
run, no explicit duplicate. No budget/threshold/profile or production release
change is authorized.


## Original-source receipt

Plan commit `51358649777` precedes the new dedicated target. On unchanged a109
runtime, thirteen tests execute: five intended timing assertions RED, eight
working controls GREEN, zero ignored. The explicit progress mapping is still
Control::Other; no runtime enum/hook or venue code has been edited.

RED boundaries are survival beyond the original deadline, final ACK release
inside the renewed window, renewed-bound live twin, canceled-watch custody, and
one accepted challenge versus duplicate renewal. Assertions beyond each first
failure remain source-stage gates. Working controls pin delayed OPEN, original
equality before callback, empty challenge/pong, normal final auth release,
default no-auth behavior, snapshot-bound isolation, genuine reconnect epochs,
and nonfinite clock rejection.

The first fixture compile used TimerId in an ordered map; corrected to the
existing hash-key contract before executable evidence. An added clock probe
showed that regressing finite host timestamps are not rejected by the existing
socket clock gate; the plan now retains its existing monotonic host-clock
contract rather than adding an unrelated chronological ledger. That exploratory
assertion is not part of the five adoption REDs. Scoped source-stage strict
Clippy and reviewer/API clearance remain pending. No push or CI is claimed.

## Source checkpoint

Runtime consumes the reviewed payload-free Progress evidence only for a current
Required-auth Authenticating feed. The private Supervisor hook computes the
candidate watch using a prospective auth origin, checks finite advancing auth
arithmetic and the existing expiry predicate, then allocates the new watch
before replacing the old watch/origin. Resource exhaustion follows the existing
fatal engine boundary with the prior bound intact. Snapshot, idle and ping
bounds remain inputs to the same watch calculation.

Progress text is forwarded to the existing native message handler after the
core control transition and a current-epoch check. Other authentication evidence
keeps its prior consumed-control behavior. This forwarding lets the native
handler retain the accepted immutable challenge and request signed auth frames;
no challenge state, reducer or signing operation is duplicated in the core.
The first implementation run exposed this missing forwarding (9 PASS/9 assertion
failures including new controls); the correction preserves every original
contract assertion and yields 18 PASS/0 ignored in the dedicated target.

Five additional actual-Engine controls directly exercise Progress under None,
Observe and already-Accepted policies, parked provider release immediately before
and at renewed equality, completed-provider delayed final drain at equality, and
old signer completion after genuine reconnect. Three private unit controls pin
invalid/non-advancing clocks, overflow and exhausted timer identities. All three
pass; the exhaustion control also proves the engine is permanently failed and
cannot drain partial output. No venue/host/signing/HTTP/map changes are included.

Scoped socket regressions pass 130 tests; the independent snapshot-barrier target
passes all 16 controls. The first normal bans run found the newly landed #2074
Breakout inventory ahead of this branch, so freeze must first carry that actual
landing and rerun normal ratchets. No exemption or foreign map repair is used.
