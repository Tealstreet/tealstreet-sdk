# Whole Phemex socket family on the shared managed core

## Checkpoints and scope

Owner: codex-5c57jl. Resume the preserved public WIP `254f55689a7` without
rewriting its history. Plain merge `2b76c4bd83f` adds frozen shared handoff
#1943 `2f7102ac0e36c5ece781a50dbbf0e6683eaad88d`. The old public-only plan
and original REDs `861462871e3` remain historical evidence. The publishable
unit converts BOTH public and private sockets; no mixed legacy/managed ingress.
Bitunix stays excluded under Sam's option 1.

Production ownership: Phemex `src/socket_codec.rs`, public connection fields and
socket hooks in `src/lib.rs`, private socket connection/auth/snapshot supervision
in `src/account.rs`, Cargo feature opt-in, dedicated socket tests and narrowly
adapted native fixtures. Keep financial reducers, account reconciliation,
request/signing builders, credential policy, history/income/trading and wire
modules unchanged. No edits to #1943's engine/host/OKX files in this adapter.

REST owner codex-ih35js confirmed boundaries: `history.rs` and the new
`tests/request_context_bounded_history.rs` are theirs; future `Pending::History`
response/failure/typed-HTTP arms are theirs. Socket Pending arms are separate.
Both owners coordinate exact adjacent lib.rs hunks before production. Keep
unsigned/signed HTTP selection and landed managed candle/error hooks intact.

## Native public contract

Retain the public WIP's one FeedId, Public role, AnyText/ControlPolicy::None,
10s connect, 20s idle, 5s ping, 1s to 30s retry and 30s stable reset. Fixed
packed ticker, active book and candle interests use native JSON and RPC IDs.
Accepted opening releases fixed ticker, books, candles and then server.ping;
subscription encoding/commit precedes ping ID allocation. Native gaps in the
monotonic wire IDs are acceptable; no reused IDs or second physical allocator.

Refcounts, 19 dynamic-interest limit, symbol/resolution checks and unsubscribe
bytes remain exact. Book HTTP seeding starts on the first accepted depth frame,
not Ready; book buffer/generation/retry/deadline stays native. Shared/Disabled
suppresses public transport without suppressing a configured private socket.

## Native private contract

Private role uses the same ws URL and existing login recipe. Connect timeout is
15s. Idle is 12.5s with ONLY native pong evidence refreshing it; ordinary data
and malformed messages do not keep the socket alive. Ping starts on accepted
open, then every 5s. Retry is jittered 1s, 2s, 4s, 8s, 16s cap; stable 30s
resets failure count. Retirement clears authenticated/physical identity and
invalidates current stream readiness before reconnect.

Opening sends exact native user.auth with id999, then server.ping id1201.
Successful id999 status=success sends aop_p.subscribe id1202. A duplicate ACK
must not become another subscription authority. Native failed auth reports the
existing AUTH_REJECTED notice and closes/retries, without invented terminal
startup policy or an HTTP fallback. The credential/signing recipe stays exact.

The 20s snapshot bound is measured from CONNECT, including time before open.
Auth ACK, pong and incremental data do not finish or renew this bound. Only
an AOP snapshot accepted by the existing account row/sequence/revision checks
ends it. A malformed/rejected snapshot cannot end the bound or gain publication
authority. Keep account REST clocks, all financial state and native staged
snapshot/fill/terminal proofs exact. Current accepted snapshots and later valid
increments retain their financial results; stale/expired epochs cannot publish.

## Shared mapping audit and prerequisite

The existing FeedPolicy auth timer begins at OPEN. Control::Auth(Accepted)
ends it before the account decoder runs. Neither alone represents Phemex's
connect-origin, accepted-snapshot bound. Copying row proof into socket_control,
using a venue snapshot timer/epoch ledger, treating login ACK as snapshot proof,
or silently extending to open+20s is forbidden.

Before private production conversion, agree with the shared owner on an existing
or additive SHARED contract for (1) a connect-origin snapshot/auth bound and
(2) accepting the terminal native snapshot proof from the exact current delivery
after existing staged validation. The shared core owns timer, generation,
retirement and backoff. Plan/auth readiness must allow native login->AOP frames
without discharging snapshot expiry early. This is an explicit mapping gap,
not permission to edit #1943's files. Original RED/control work proceeds while
the shared owner reviews it; no production workaround is publishable.

No new Ready-to-HTTP obligation was found: initial account reads begin at
startup and public depth initiates its existing seed. If an actual subscription
prerequisite is found, use #1943 released-interest custody and real timer ingress;
never a venue defer queue. Shared handoff refusal/retirement cannot lose a native
obligation, and a callback must choose fresh native state at execution time.

## Sequential phases and executable evidence

1. Commit this whole-family plan and self-audit paths/policies; request bounded
   REST coupling and shared lifecycle reviews. Existing public WIP is not a
   cleared production result.
2. Commit assertion-only private real-Engine controls against identifiable
   ORIGINAL private production. Use an isolated git archive of `861462871e3`
   with the dedicated test added; do not mislabel the public-only WIP swallowing
   private ingress as a genuine native lifecycle RED. Keep the archive source
   pin and command in the receipt. Tests must compile/run; fixture failures
   and failed optimistic commands are separate outcomes.
3. After plan/seam clearance, adopt the entire private transport and reuse the
   public projection. Delete old private Supervisor/raw SocketId authority,
   connect/auth/ping/retry timer ownership. Native authenticated/snapshot flags
   remain financial protocol facts; the core alone owns delivery permission.
4. Re-run original REDs, whole-family positive cases, existing account and
   supervision suites; adapt fixtures only to genuine open/close/retry/current
   auth/snapshot transitions, preserving assertions. Audit and commit fixes.
5. Integrate actual #1943/train landing, run strict scoped checks and fresh CI;
   one venue per PR, train slot and separate guard before any merge.

Original private RED targets: duplicate accepted open does not resend login;
late open after15s cannot release login; an accepted-looking snapshot after
connect+20s cannot publish when watchdog delivery is delayed; an order frame
after12.5s idle cannot publish; a late pong cannot revive an expired epoch.

Working controls: exact login/ping/AOP order, live accepted snapshot and native
financial values, connect-origin deadline despite delayed open and pongs,
malformed snapshot retaining expiry, genuine reconnect and old epoch inertness.
Retain existing public REDs/positives and add whole-family Shared/Disabled,
auth-refusal, duplicate ACK, stable reset, current-vs-expired financial and
snapshot-before/at-boundary controls. No ignored tests or weaker assertions.

## Inventory and performance gates

Whole-family conversion removes only Phemex's original socket rows: public
lib.rs bfc950bbe21c9d625ea9774177bfa8606bb559d376d71b7ad05a25603c20683a
and private account.rs c42cbb663aa3220ad6b6ccb5c064414482645c5a658b8ed112350d89b70625ff,
one occurrence each, plus only its legacy family root. Public WIP already
pruned the public row; final proof compares against original landing and the
current scanner. Preserve every other venue and the HTTP/signing inventories.

Commands: scoped Phemex tests and strict all-target/all-feature Clippy,
formatting, existing bans/ratchets and automatic engine feature-mode closure.
Fresh CI must meet unchanged Phemex public/private CPU, retained memory, size
and replay gates. No profile/reference/threshold/harness changes. Native tests
are correctness evidence only. Record actual results and review scope in the
PR body; CALLERS describes landed behavior. Temporary managed-sockets remains
a shrink-only remove-before-master obligation, including excluded Bitunix.

## Executed original private controls

Isolated native production: git archive `861462871e3` (before public adapter),
with only `tests/managed_private_socket.rs` added. Exact lib/account/sign/history
and Cargo bytes match that commit. Command: `cargo test -p
safe-cex-venue-phemex --test managed_private_socket` in the archived sdk-rs.
All ten compile and execute: SIX genuine assertion REDs, FOUR working GREENs,
zero ignored. Duplicate open emits4 frames instead of2; expired connect emits
a login; duplicate auth emits2 AOP subscriptions instead of1; deadline-equality
snapshot and idle-expired order publish; idle-expired pong fails to retire.

The four native positives pin full auth/signature/expiry/ping/AOP bytes, accepted
financial snapshot, connect-origin20s expiry despite delayed open/ACK/pongs,
and malformed snapshot retaining expiry. The initial test run had one fixture
field-order error in the new auth expectation; corrected to the native exact
serializer bytes before this receipt. No native production was changed.

Shared owner confirmed no reviewed completion/barrier seam expresses the
connect-origin accepted-snapshot contract. The separate prerequisite must retain
independent login and snapshot facts; exact current-message epoch custody
discharges the barrier only after native validation, with equality expiry
before publication. Stale/borrowed/duplicate completion is inert; pongs remain
independent liveness. Ownership/API ruling is requested before source.
