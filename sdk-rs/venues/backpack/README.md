# Backpack perpetuals

The sans-I/O venue engine runs visible PERP markets on Backpack. The SDK selects
it with `engine: 'rust'`. REST and private websocket authentication use an
Ed25519 seed encoded as base64. REST receive windows default to 60,000 ms and
clamp to Backpack's maximum; subscription signatures use 5,000 ms.

SDK/registry construction consumes keys and seeds into the owned provider and
passes only opaque signing refs to the venue, including for account startup and
private WS. Hosted RemoteSigner uses the device's Ed25519 capture codec; private
PayloadSigner remains unsupported. This activation is held for Sam's approval:
see `docs/plans/callers/backpack-hosted-signing.md` at the repository root.
Direct raw-options construction retains its existing compatibility path.

Caller client IDs are canonical decimal strings from `1` through `4294967295`.
They are sent as JSON uint32 numbers. Preflight rejects unsupported shapes and
invalid IDs before any write. Amend cancels and replaces, preserving trigger
source and execution limit, and never reuses the original caller ID. A lost
write response is not retried. Post-only and time-in-force flags survive
reconciliation, amend and trigger repair. Attached protective stops remain market
orders, including after a gap.

Split dispatch follows the legacy TypeScript adapter in
`packages/safe-cex/src/__tests__/reference/backpack/backpack.exchange.ts`.
Legacy rows round without rechecking wire quantity bounds or native maximum
prices. Independent rows, pairs, cancel batches and protective children continue
after a proven refusal; Unknown stops the remaining writes. A failed entry skips
its own protective children.
These inherited partial-placement cases remain known issues for Sam.

Prepared protective put-backs retain the same rounded quantities without an
extra rounded-wire bounds check.

The engine's `WritePlan` observes dispatch and reports indexed outcomes without
adding a validation gate or choosing continuation. Proven placements retain
native IDs, ambiguous writes remain unknown, and skipped children are not sent.
The command's terminal error retains the journal and accepted IDs after the
legacy loop finishes. A failed write is never resent.

Accepted cancel and leverage effects use `Applied`, with existing cancel targets
kept separate from newly created IDs. Legacy cancel-replace uses the requested
total and edited price after cancellation, without a fill or reference veto.
Protective put-back kept per Sam's decision, SPLIT_PLAN_INVARIANT.md exception 2:
a definite coded refusal of a protective replacement can read open orders and
put back the untouched original once, only after proving the replacement absent.
Unknown replies stop without another write. Background missing-trigger-source
repair continues its separate legacy cancel/re-place behavior.

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
