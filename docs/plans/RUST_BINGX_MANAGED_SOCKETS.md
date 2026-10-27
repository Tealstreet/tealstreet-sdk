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
