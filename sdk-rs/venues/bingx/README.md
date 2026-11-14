# BingX perpetual swaps

The independent native/fixture-only `bingxspot` variant is described in [BingX Spot](SPOT.md).

A typed sans-I/O venue for `safe-cex-engine`, supporting stablecoin-margined
perpetual swaps. Public snapshots replace every supplied depth level; the
internal public feed sends descending asks, which are sorted before delivery.
Account polling supplements the private stream. Listen keys recover separately
from socket liveness, with epoch guards on old requests and timers.

REST open-order rows with readable identity, symbol, side and quantity remain
visible and cancellable when optional fields fail to parse. Diagnostics name
the malformed fields without their values. Degraded rows do not authorize
amendments or clear uncertainty fences; a clean snapshot restores metadata.
Private execution frames retain their strict decoder.

Hosts connect directly to REST `open-api.bingx.com`, internal public WebSocket
`ws-market-swap.qq-os.com`, and private WebSocket `open-api-swap.bingx.com`.
Gzip/deflate frames are inflated by the browser/Worker/native transport before
entering the core. No Tealstreet proxy/tunnel is used. Browser REST requires
permitted CORS egress or a transport on a server/Worker; API allowlists must
include that host's direct egress IP. The internal public host is not BingX's
documented public protocol and remains a deployment dependency for TS parity.

Writes are sequenced and never retried after transport failures. A refusal
BingX makes before processing, a bad signature (100001) or a stale timestamp
(109400 naming it), is re-signed and resent once on the initial write; later
ordered writes never resend after an accepted prefix. A timestamp resend reads
BingX server time and recalibrates this account's signing clock. Batch inputs
are formatted before the first write, retaining the existing input rules. Caller IDs are honored verbatim on
one ordinary order or rejected before writes. Leverage changes without a side
update both hedge sides sequentially, matching the engine's symbol-level API.

Legacy TS formats all placement rows before sending them, but does not validate
every lot or TP/SL child for notional, require a MARKET last-price reference,
or impose a combined native-write count limit. Ordinary quantity amendment
checks normalized quantity bounds; atomic price replacement leaves its new
notional and quantity rules to the venue. These known partial-placement cases
retain the pre-existing behavior rather than adding a new validation gate.
Protective put-back, restore and replacement behavior is unchanged.

Lot allocation follows `bingx.exchange.ts:1544-1564`: take
`ceil(amount / maxQty)` equal rounded lots, then append the rounded native
remainder when nonzero. Each quantity is rounded independently, so their sum
can differ from the normalized requested amount. Recording tests in
`tests/trading/allocation.rs` pin the six audited sequences, including integer,
half-step and thousandth-step cases.

Attached SL and TP each send once at the normalized entry amount, independently
of the entry lots (`bingx.exchange.ts:1567-1595`). Failure text counts accepted
entry lots from the journal: accepted children never confirm a refused entry,
and an accepted subset is described as a partially placed entry.

Prepared native writes use the shared `WritePlan` journal. Failures expose an
ordered `elements` array covering each placed, refused, unknown and unsent lot
or child, with exact client/native identities. Accepted cancellation, amendment
and settings effects use `Applied`, without claiming newly created order IDs.
Like the perp TS loop, later lots, batch rows and TP/SL children continue after
an individual definite refusal. A genuine Unknown halts further writes for the
action, including independent cancels, and asks the user to check Open Orders.
This is the recorded stop-after-UNKNOWN parity exception. Native confirmed-
cancel/definite-replacement-refusal restore stays kept pending policy. The
shared journal only reports facts; it never validates or schedules writes.

An ordinary cancel answered `109400` with exactly `order not exist`, or `109421`,
uses the same satisfied-cancel path as `80016`: the command completes, but its
symbols remain fenced until a fresh primary-key order snapshot reconciles them.
Edits and protective cancels require their existing cancellation proof; a missing
order reply never authorizes a replacement or put-back. Generic `109400`
parameter and timestamp refusals keep their existing classifications.

The gate declares TS fixes for zero decimal precision, descending asks,
non-finite ticker placeholders, per-execution fills and terminal dedupe,
`cp` close flags, required full-position stop quantity, fail-closed bracket
sequencing, contradictory ACKs, trigger-source preservation and checks, and
fill-history query parameters. Tests derive
from BingX's published payloads and the SDK's documented runtime contracts.

Socket recovery uses independent engine `Supervisor` instances. Both bound
connection and listen-key acquisition to 10 seconds and use capped exponential
backoff with deterministic jitter that survives open/close flaps. Public silence
expires after 20 seconds, with `Ping` every five seconds; private silence expires
after 45 seconds, with `Ping` every 15 seconds. Native `Ping`/`Pong` replies remain
unchanged. Private expiry and watchdog recovery wait for backoff before POSTing
a fresh key. Failed token acquisition retains a minimum 30-second retry pause.
Retired epochs fence late token and keepalive responses.

History read refusals retain a typed kind, the numeric venue code, and a closed
`causeCode` (rate limit, key/permission, invalid parameters, venue failure, or
decode). These diagnostics do not change one-resend or strict history recovery
rules. Code meanings follow [BingX's error reference](https://github.com/BingX-API/api-ai-skills/blob/main/skills/references/error-codes.md).

Protective edits retain native closing authority and use a final managed-Claim
veto with a genuine <=5 s Mark/Last receipt. Proven standalone stop-market edits
use native cancelReplace; explicit partial refusal permits one exact put-back.
Ambiguous replies and guarded rate limits are Unknown, with no resend or restore.
A missing or stale requested receipt triggers one unsigned Daily (Last) or
Premium (Mark) preflight read, bounded to 20 s with no retry, before destructive
admission. The target row retains its actual receipt time and is usable only
after the whole envelope succeeds, within the unchanged 5 s freshness limit.
Frame-fed edits use this same path; frame arrival never refreshes a reference.
A failed, sparse, wrong-source or delayed read leaves the original open. Final
Claims, re-signs and restores still use the same pure veto. Only the veto is
I/O-free; re-signs and restores retain their existing server-time reads and writes.
See [protective edits](../../../../docs/plans/callers/bingx-protective-edits.md)
for the native classes, proof requirements and frame-state version contract.
The genuine <=5 s per-source proof is intentionally stricter than legacy TS,
which optionally checked Store Mark without a receipt-age gate.
