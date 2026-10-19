# BingX perpetual swaps

A typed sans-I/O venue for `safe-cex-engine`, supporting stablecoin-margined
perpetual swaps. Public snapshots replace every supplied depth level; the
internal public feed sends descending asks, which are sorted before delivery.
Account polling supplements the private stream. Listen keys recover separately
from socket liveness, with epoch guards on old requests and timers.

Hosts connect directly to REST `open-api.bingx.com`, internal public WebSocket
`ws-market-swap.qq-os.com`, and private WebSocket `open-api-swap.bingx.com`.
Gzip/deflate frames are inflated by the browser/Worker/native transport before
entering the core. No Tealstreet proxy/tunnel is used. Browser REST requires
permitted CORS egress or a transport on a server/Worker; API allowlists must
include that host's direct egress IP. The internal public host is not BingX's
documented public protocol and remains a deployment dependency for TS parity.

Writes are sequenced and never retried after transport failures. A batch is
fully preflighted before any order is sent. Caller IDs are honored verbatim on
one ordinary order or rejected before writes. Leverage changes without a side
update both hedge sides sequentially, matching the engine's symbol-level API.

The gate declares TS fixes for zero decimal precision, descending asks,
non-finite ticker placeholders, per-execution fills and terminal dedupe,
`cp` close flags, required full-position stop quantity, fail-closed bracket
sequencing, contradictory ACKs, trigger-source preservation and checks, and
fill-history query parameters. Tests derive
from BingX's published payloads and the SDK's documented runtime contracts.
