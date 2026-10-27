# BingX managed socket lifecycle

One venue-only PR on `origin/master`, followed by BloFin. No merge before SDK
#2300 enables master Rust CI. Every merge requires 8x5tm1's exact-tree GO.

Both public and private feeds register before either connects. The engine owns
physical epochs, connect/idle deadlines, ping cadence and reconnect backoff.
Public supervision remains 20s idle / 5s ping; private remains 45s / 15s.
Both use AnyText liveness and AuthMode::None: the private URL contains an
accepted listen key and has no authentication ACK. Ping/Pong wire handling
remains native; there is no fabricated authentication evidence.

Public interests are market.contracts plus refcounted depth/candle topics.
The codec emits one exact native frame per interest with monotonically checked
IDs. Desired-interest replacement is transactional: synchronous admission failure
restores local demand; asynchronous Capacity retires the affected public epoch,
retains desired demand and lets engine backoff/replay recover. Permanent invalid
interest/encoding errors stop only the affected feed. Initial Ready replay
failure inherits the engine's fail-closed disposition. No HTTP book handoff is
needed: BingX publishes full-depth stream snapshots. Retiring public clears its
books and publishes invalidation before a replacement can deliver rows.

Listen-key creation/keepalive retains its genuine managed Session HTTP path.
Private retirement invalidates that session and key once, cancels keepalive,
bumps native snapshot revisions and waits for the engine's reconnect grant.
That grant obtains a fresh key before connecting. HTTP acquisition failures
before any physical epoch retain a single native 30s HTTP retry timer; they do
not create socket reconnect authority. HTTP's own 20s deadline bounds a hung
key acquisition rather than a parallel socket supervisor's 10s watch.
Private Opened performs native Ping, keepalive and account refresh once.

OrdersRead records `cx.order_read_epoch()` at its own issuance and clears via
`orders_reconciled_since(None, epoch)` after clean accepted Store publication.
Existing native revision and malformed-row guards remain in force.

Production-bound controls use Bingx with genuine installed providers, actual
physical IDs and managed HTTP Prepare/Bind/Claim. Cover duplicate/stale/nonfinite
Opened/message/close/timer delivery; independent sibling feeds; public
suppression with private success; subscription refcounts and replay of current
demand; private fresh-key reconnect and keepalive ownership; real pressure and
refusal recovery; order-fence freshness. Preserve existing financial assertions,
wire literals and account-read/trading behavior. Any fixture clock/setup changes
are explicit. Native correctness, strict clippy, fmt, bans, CI family/replay
records and auditor GO gate landing. No local family or combined build unless
required by a new ruling. Inventory changes remove only migrated BingX sites.

## Local checkpoints

The original real-binding controls produced 2 working sibling/suppression tests
and 2 intended duplicate-Opened assertion failures. An earlier fixture version
first failed to compile on the wrong policy enum name, then failed before the
private duplicate assertion because its startup Orders request remained held;
those setup failures are not counted as regression RED evidence.

The candidate's nine lifecycle controls pass, including a real 2,100-book
metadata remap that exceeds transactional frame capacity. The public book is
invalidated, private Ping/Pong survives, and engine retry recovers all retained
books plus the new candle interest. The key-acquisition retry control drives
its actual early/rearmed/equality/duplicate timers and rejects a retired key
answer before accepting a newly issued key. Current-source full native, strict,
fmt, bans, immutable source review and master PR CI remain separate gates.

The source review found that an ordinary acquisition-retry timer is consumed
before a nonfinite venue callback. A real captured-timer regression reproduced
the resulting lost wake. Retry custody now retains its original finite bound
separately from the optional physical timer. Nonfinite delivery clears only the
consumed identity; the next genuine finite account timer, lifecycle or live
read callback rearms once at max(original bound, current time). No key request
is sent before the bound, and no timer is scheduled from a nonfinite clock.
All three nonfinite deliveries, a finite poll with public sockets suppressed,
idempotent rearming, early/equality/duplicate wakes and new-key connection are
covered. The core owner confirmed this existing-API composition; no engine seam
is added. Original failure and corrected execution remain separate log evidence.
