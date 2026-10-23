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
and malformed messages do not keep the socket alive. The accepted-login ruling
supersedes the original immediate opening ping: account ping/private sends wait
for accepted login; the shared periodic watch remains armed throughout. Retry is jittered 1s, 2s, 4s, 8s, 16s cap; stable 30s
resets failure count. Retirement clears authenticated/physical identity and
invalidates current stream readiness before reconnect.

Opening sends exact native user.auth with id999 and reserves the native opening
ping id1201 in shared send_after_socket_interests custody. Successful id999
status=success permits the encoded aop_p.subscribe id1202, then the retained
server.ping id1201. Before ACK, neither AOP nor account ping may leave. This is
an intentional order change from the original auth,ping,ACK,AOP sequence;
all individual JSON bytes and RPC identities remain pinned. A duplicate ACK
must not become another subscription authority. Native failed auth reports the
existing AUTH_REJECTED notice and closes/retries, without invented terminal
startup policy or an HTTP fallback. The credential/signing recipe stays exact.

The 20s snapshot bound is measured from CONNECT, including time before open.
Auth ACK, pong and incremental data do not finish or renew this bound. Only
the accepted orders_p snapshot branch in account.rs (original1074-1079)
ends it. Positions/accounts-only snapshots do not. An accepted orders snapshot
with safe_snapshot=false from a newer order_revision still ends the barrier
while preserving newer financial state; revision equality is not required. A malformed/rejected snapshot cannot end the bound or gain publication
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

## Native completion-hook precision controls

Bounded REST-owner review identified the exact native orders_p snapshot hook;
the plan now names it instead of treating every AOP snapshot as completion.
Two additional real-engine working controls run on the SAME native861462
archive: positions/accounts-only snapshots retain connect-origin expiry; an
accepted revision-conflicted empty orders snapshot preserves the newer order
and ends the barrier, allowing pongs/current state beyond connect+20s.
Expanded receipt:12 executed,6 original assertion RED,6 working GREEN,0ignored.
The original ten-control receipt remains identifiable at81fc6001a6b.

## Reviewed prerequisite and accepted-login adoption

Plain merge4e679fd493a stacks reviewed #1970 headde8106fd6d0, including actual
#1943 and current venue landingf324. Engine/host/OKX blobs match that prerequisite
exactly. The dependency is unmerged; actual1970 is required before final consumer
proof/merge. Shared-doc conflicts retain the incoming documents with the Phemex
plan index row appended once. Incoming credential-selection whitespace is kept.

Overseer explicitly requires no account ping/private subscription before login
ACCEPTED. Original accepted_open_preserves_auth_then_ping_and_aop_wire remains
identifiable atc8812e4e85a (and the original native861462 archive). The next RED
is the intentional policy change: opening must emit auth alone, then accepted
login emits AOP followed by the shared queued ping. When that RED is adopted,
update the older positive's order assertion explicitly; preserve all auth,
signature/expiry, ping and AOP byte assertions. No implicit assertion weakening.

Private transport mapping: one FeedId with the declared snapshot policy,
PongOnly traffic and a native control classifier limited to id999 auth proof and
native pong evidence. No financial payload predecode. Shared Opened releases
only login and retains the opening ping through shared one-shot custody;
Authenticated(first) updates the native authenticated fact and proposes the
fixed AOP interest. The immutable connect-origin snapshot barrier remains live.
Shared Ping uses the native checked RPC formatter; shared Reconnect reacquires
the same feed; Retired clears native open/auth/snapshot facts. Remove the private
Supervisor, connected_at, raw SocketId and legacy timer/send ownership. Native
orders_ready continues to depend on accepted-open/auth/snapshot facts; none is
transport admission authority. Whole-family feature mode must include Phemex.

The account reducer retains one existing fallible prepare/validate path. Once
all known-row validation and position preparation succeeds, but before sequence
or any Store publication changes, an accepted orders snapshot calls shared
complete_socket_snapshot. Failure returns without publication. Positions-only,
accounts-only and rejected snapshots do not call it; accepted revision conflicts
do, preserving the original safe_snapshot behavior. No financial code/table,
request builder, signing recipe or REST callback change belongs in this slice.

## Whole-family implementation checkpoint

Production6a03237b0c8 removes the private Supervisor/rawSocket/connected_at and
uses the reviewed snapshot feed while leaving HTTP/sign/history/trading/wire
source untouched. Scoped dedicated controls13PASS after the explicitly ruled
ping-order update. Full native216tests across15targets PASS after d34842a08ea:
the recurring-ping fixture supplies a genuine accepted ACK, opening-ping control
asserts zero before ACK/one after, and stable-stream checks compare against the
known closed-retired count before the current connection. Existing financial
assertions are unchanged. The first optimistic edit used an incorrect cwd-relative
path and changed no source; the corrected edit and result are recorded here.

Original accepted-login REDb265d081c07 executes on the same immutable native861462
archive and fails with one opening ping instead of zero. No compile failure is
counted as RED. New controls add Shared/Disabled private financial publication,
prelogin denial with a current accepted positive, and old-epoch denial alongside
a real new-epoch orders snapshot. Source-review, actual dependency landing,
strict checks and full CPU/memory/size/replay CI remain separate gates.

## Conservative request inventory precision

The lexical request checker also froze the old public RPC counter named request:
lib.rs f9764ec2239f809efce38c8d387de2eb9e144668ccf4e1d4193e1859c5e5e05f
(two member references) and8e74b914b9159816adc7eb353d1c728eaea647260ac64fac31e4b967a6dab017
(one format macro containing that field). The managed public formatter names
that Cell rpc_id to describe its native purpose. Prune only those three removed
lexical occurrences; this is NOT an HTTP RequestContext migration or new allowance.
Every actual HTTP builder/legacy call remains unchanged. Exact public wire-ID
controls prove the counter still starts at1 and preserves accepted send order.

## Independent checkpoint and scoped verification

At893d3241d1d, shared-owner API/custody source review is CLEAR and an independent
immutable git-archive run passes private16/16 plus public9/9,0ignored. The REST
owner independently clears native financial/HTTP/wire/fixture preservation at
the same checkpoint without a test-execution claim. Later15a3 is a public-codec
collapsible-if lint cleanup;76a0 is the explicitly documented RPC-counter naming
and lexical prune. Actualcab37 session landings are genuinely integrated atf61b.
Current scoped native219tests/15targets, strict all-target/all-feature Clippy and
full bans/ratchets PASS. No actual1970, CI/performance or train-ready claim follows
from these local/independent results. Full fresh CI measures unchanged budgets.


## Combined socket and signer landing

Per the owner ruling, #1974 now includes reviewed signer handoff
`6ff43a058d98b47c5166da74e5d61554cad5587b` through plain merge
`3d5752624cc19a39d7c5c918fd9225ac25b7a77d`. Its whole tree equals the
handoff tree; no merge resolution changed source. The frozen #1970 prerequisite
remains `04b0078dd29` and must land first. Actual incoming `6c7f4fcf292d`
contains the unsigned PublicTrades conversion, preserved byte for byte.

The direct Phemex lib.rs inventory boundary is incoming 17 fingerprints/18
occurrences to combined 16/16. The old adapter-only 15/15 checkpoint will not
land separately. The two approved request-checker literal rows and Phemex
signing-scan scope remain exact to the reviewed signer implementation; request
checker AST is unchanged after removing those two literal assignments.

The codec is byte-identical to reviewed adapter85fa; the signing change in
account.rs is confined to the same-epoch Opened auth recipe. Accepted login
still releases AOP1202 before retained ping1201, and the native validated orders
snapshot remains the only financial completion proof. HMAC/full-map receipts
are in RUST_PHEMEX_DISPATCH_SIGNING.md. The shared core is unchanged by this fold.

Fresh combined local verification at merge3d575: 259 Phemex tests across16
targets PASS,0failed/ignored; strict all-target/all-feature Clippy and scoped
rustfmt PASS. The dedicated SDK construction test passes. The initial local
ratchet invocation at the merge commit compared its adapter-only first parent
85fa15/15 against combined16/16 and refused; that is not the intended actual
landing boundary. Final checked PR descendants and CI must retain the direct
actual17/18 comparison with no new predicate or intermediate landing.
Fresh combined CI and independent cross-review are separate publication gates.


The combined secret scan found one additional historical occurrence of the
same synthetic HMAC-SHA256 vector in signer RED77f3a83f9fa9,
tests/dispatch_signing.rs line424. Python hmac independently recomputed it
from literal key/secret and payload key1700000060. Only that exact historical
commit/path/rule/line is allowlisted. Gitleaks8.30.1 scans the complete42-commit
actual6c7..a487 PR range clean after the correction. A separate unallowlisted
synthetic high-entropy credential still produces one finding. An initial detector
probe with a TEST_ONLY prefix was ignored by the scanner's existing default
fixture policy, so it was replaced by this independently working control. No
runtime, test assertion, signature value, rule or path-wide allowance changed.
