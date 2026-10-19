# Backpack perpetuals

The sans-I/O venue engine runs visible PERP markets on Backpack. The SDK selects
it with `engine: 'rust'`. REST and private websocket authentication use an
Ed25519 seed encoded as base64. REST receive windows default to 60,000 ms and
clamp to Backpack's maximum; subscription signatures use 5,000 ms.

Caller client IDs are canonical decimal strings from `1` through `4294967295`.
They are sent as JSON uint32 numbers. Preflight rejects unsupported shapes and
invalid IDs before any write. Amend cancels and replaces, preserving trigger
source and execution limit, and never reuses the original caller ID. A lost
write response is not retried. Post-only and time-in-force flags survive
reconciliation, amend and trigger repair. Attached protective stops remain market
orders, including after a gap.

Account leverage is global. Position mode is fixed hedged, and margin mode is
fixed cross; requests for another mode return an error. Private reconciliation
runs every ten seconds. Failed reads preserve prior state; stream revisions
protect newer data from delayed snapshots. Execution IDs suppress duplicate
fills. Missing trigger sources on existing protection orders are repaired by
sequenced cancel/replace jobs.

Ticker statistics support documented opening-price/quote-volume fields. Best
quotes come from bookTicker; partial depth changes never replace them, and null
book sides clear stale quotes.

Failed startup reads retry independently with bounded exponential delays. The
SDK uses its synchronized clock for Backpack authentication and supports custom
class-based transports.

Depth snapshots accept exact string or integer sequence IDs. The first buffered
update must bridge the snapshot; later updates must begin at the next sequence.
Three consecutive failed recovery attempts clear only the affected book.

History reads return fills, orders, funding payments and public trades. Other
income ledgers return an explicit unsupported error. Pagination rejects repeated
pages and stops with an error if the bounded walk cannot complete.

Tests use documented payloads and public deterministic signing keys. Generate
the private replay tape with `corepack yarn -s rust:private-tape-backpack`; the
public fixture comes from a read-only live recording. Gate corrections for TS
mapping and throttling defects are declared in `../../harness/gate.mjs`.
