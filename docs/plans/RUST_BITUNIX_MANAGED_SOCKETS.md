# Bitunix managed-socket adoption

Refresh owner: strix/codex-9ui18z for #2301, at claude-psmp2r's direction.
The original implementation was by claude-4yjlw0. This refresh starts at its
`a242fc5e9c2bad97b7ae5a7ebb73a753484473f9` head and normally merges mirror master
`bdb7565ebde53e38266b00de9d52d4126c1a8f23`, followed by a clean ordinary merge
of current master `a4ab0ee7022257e4bcc7ceb106a65d45b29fd479`; no rebase or
history rewrite.
Fresh independent reviews are required; the October plan reviews do not approve
this composition. The changed `sdk-rs` inputs wait for a release batch.

## Scope

Move futures, public shards and the private socket from venue-owned Supervisor,
queues and login-retry timers to engine-owned socket epochs and interests.
Retain current master account/reconciliation/trading logic and the legacy TS wire
reference. Remove only Bitunix's family and five sites from the socket inventories;
all other families and inventory fingerprints remain as on merged master.

The engine owns connection/auth/idle deadlines, backoff, stale-event refusal,
subscription release and the coalesced two-second private login retry. The venue
still owns wire codecs, account reducers and feed-specific policy. No signing,
credential, financial mapping, order-write or retry classification policy changes.

## Feed policy

| Feed | Authentication | Receive evidence | Other policy |
| --- | --- | --- | --- |
| Futures (`ws-futures`) | None | Any text, 45s idle | 10s connect; 15s ping; clear books and notify `book_changed` on retirement. Three fixed epoch-bootstrap system frames precede book/candle interests; the latter retain all 4096 frame slots for the native host limit. |
| Public shards, one per 300 markets/channel | None | Any text, 45s idle | 10s connect; 15s ping; 20-symbol native batches spaced 250ms apart. A shard retirement does not clear futures books. |
| Private (`/private/`) | Observe, with a 10s Supervisor login-answer bound | Text after the first actual pong, then 45s idle | 10s connect; 15s ping; 10s stable threshold after authentication; one 2s same-socket login retry. |

All pings retain master's protocol clock. Private pings use its current
`{op:"ping", ping:<seconds>}` codec; both `op:"pong"` and the real
`{op:"ping", pong:..., ping:...}` response count as pong evidence. A private
session without an actual pong keeps its idle gate disabled. A successful login
still cancels its authentication bound; no-answer timeout warns `stream.unavailable`
and reconnects, without asserting rejected credentials.

Public shard registration is skipped when public data is shared or disabled, so
intentional host suppression emits no admission error. Caller candle spellings
remain independently desired, while equal canonical native topics coalesce into
one managed interest until the last spelling is removed. Case aliases cannot
create duplicate interest keys or unsubscribe another caller's retained topic.
Only `op` supplies private pong evidence; `ch` remains a login-dispatch fallback.

Timeout retirement carries `recover:true` to the replacement; an ordinary close
carries `recover:false`. Existing explicit proxy routing remains engine-owned.

## Private authentication and account behavior

The October branch used Observe without a login deadline. Current master added a
10s login-answer bound, the private ping codec, a 10s private stability threshold
and successful-login balance pacing. The refresh preserves those newer contracts.
Observe allows account data before an answer, matching current legacy reduction;
the Supervisor independently refuses expired ingress at the answer deadline.
Required would instead gate pre-answer data and is not part of this migration.

| Evidence | Engine action | Venue action |
| --- | --- | --- |
| Accepted | Cancel authentication and pending retry | Mark login answered and private live; keep the current 5s balance-poll cadence. |
| Explicitly rejected | Keep the original answer deadline and one latched 2s retry | Mark answered and emit the existing `auth.rejected` message. |
| Malformed | Same bounded, latched retry | Mark answered and emit `stream.malformed`, without a bad-key claim. |
| No answer | Retire at 10s, including ingress before timer delivery | Preserve the existing unanswered-login warning. |
| Retirement | Cancel retry with its epoch | Clear private-live state, restoring ordinary balance polling. |

Every private open sends ping then signed login and requests balance, positions
and orders. Existing read serials, dirty-overlap handling and two-half readiness
coalesce/reissue those reads exactly as on master. Positions still trigger TP/SL
reads. Balance notifications retain their forced-read flag and first repair
bound. Accepted private login slows ordinary balance polling to 5s; retirement
restores the normal 500ms cadence. No old financial state is overwritten by the
socket migration.

## Verification

The production-binding `tests/managed_socket.rs` drives the real engine. Current
master is the baseline, with only those new tests overlaid:

- An otherwise valid futures frame at the 45s idle boundary must not publish.
- A private order frame at the 45s idle boundary after real pong must not reduce.
- A private order at the 10s unanswered-login boundary must not reduce or revive
  its epoch, even before the deadline timer is delivered.
- Controls retain duplicate-open refusal, pre-open refusal, one-time replay,
  ping-before-login, pre-answer account data before 10s and public-shard pacing.

Run the full current Bitunix suite, including login controls, socket lifecycle,
live balance pacing, routing, account-read progress, ordinary order reconciliation
and trading/signing uncertainty tests. Retain all existing assertions; fixture
updates must model live sibling feeds and actual current replacement sockets.
Registry Bitunix tests, package format/strict lint, all configured bans and the
socket checker's own tests remain required.

Exact-head `ci`, required coverage, Bitunix replay correctness and actually
executed combined consumers are required before requesting the two fresh
reviewers. Retired CPU certification/gate jobs are not restored. Fresh optimized
Bitunix and combined web/Node sizes must respect existing floors and the 10KiB
reserve rule. The first refreshed source `b1ac2260570b` measured optimized
web/Node 2,012,442 B in CI44266617073, exceeding its inherited 1,967,104 B cap.
The measured-size-plus-10KiB rule yields a 2,023,424 B family cap and 10,982 B
reserve; the budget regression preserves that reserve, every historical floor
and strict cap/cap+1 behavior. The correction requires fresh exact-head replay
and combined evidence. This plan does not authorize release.
