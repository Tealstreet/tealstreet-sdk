# BingX perpetual swaps

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
(109400 naming it), is re-signed and resent once; a timestamp refusal also reads
BingX server time and recalibrates this account's signing clock. A batch is
fully preflighted before any order is sent. Caller IDs are honored verbatim on
one ordinary order or rejected before writes. Leverage changes without a side
update both hedge sides sequentially, matching the engine's symbol-level API.

Every split lot must satisfy native min/max quantity, step and minimum notional
before the first write in the batch. Notional uses the actual rounded LIMIT
price, or the ticker's last price for MARKET orders; a required missing or
nonpositive reference refuses the plan. Checked decimal arithmetic verifies
inclusive notional bounds and exact quantity conservation. Invalid layouts are
refused as InvalidInput/NotSent with zero requests, without resizing or
rebalancing. TP/SL, time-in-force and reduce-only fields keep their existing
per-lot values. Explicit reduce-only quantities also obey these minima, so a
below-minimum dust intent refuses locally. The separate native full-position
close path keeps its existing position-derived quantity and close flag.

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
Frame-fed protective price edits refuse `reference stale` until the engine maps
genuine source receipt clocks; frame arrival never refreshes a reference.
See [protective edits](../../../../docs/plans/callers/bingx-protective-edits.md)
for the native classes, proof requirements and frame-state version contract.
