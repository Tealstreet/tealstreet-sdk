
### BloFin managed public and private sockets

Both feeds declare managed_sockets and register before either connects. Managed
epochs own Opened/ACK, deadlines, ping and reconnect; legacy supervisors are
removed. Private keeps the installed Primary SessionAuth recipe and requires
code0 login proof before private subscriptions and financial rows. Duplicate or
stale opens/ACK/signing completions cannot replay login or refresh. Public topics
are transactional interests with shared books5 demand, native4096-byte batches
and managed sequence repair. Public Capacity retires only public, publishes book
invalidation and recovers retained desired work; permanent defects and inherited
Ready replay errors fail closed on the affected feed. Canonical topic ordering
and avoiding redundant books5 subscriptions are intentional safer choices.

The full three-family/page order census retains the first order_read_epoch and
uses orders_reconciled_since only after clean current Primary Store publication,
keeping revision and uncertainty-generation guards. A later-page token never
freshens a pre-mark census. HTTP selection, trading and financial reducers are
outside this slice. Existing fixture lifecycle setup changes, when necessary,
are recorded in the plan and preserve their financial assertions and payloads.

BloFin reconnect proposal pressure installs and retires a genuine replacement
epoch before backoff, with no exposed connect/frame and no private-peer loss.
Fixture adaptations explicitly establish private auth locally. Two account
reread fixtures use actual reconnect instead of duplicate ACK; trading pauses
use response-relative captured timers. Financial assertions/raw payloads remain
intact; observation clocks follow their genuine new socket/timer events.

Review corrections: callers may subscribe or unsubscribe while the public epoch
is connecting or while another interest proposal is pending. The venue preserves
ordered changes and duplicate book/candle references, then recomputes desired
interests after the exact proposal commits. Disconnected demand survives into
the next epoch; a private refusal cannot mutate public proposal ownership.
Synchronous admission refusals still report and roll back only their own change.
The SDK, NX and server must continue forwarding actual socket/timer identities;
no raw sender or parallel supervisor is needed. Removal prerequisites remain the
zero-site socket census and the production-binding controls in this change.
