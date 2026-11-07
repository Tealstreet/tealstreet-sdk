# Hyperliquid critical read admission

Order-prerequisite reads and engine-owned order-fence reconciliation have critical
priority in native admission and the shared `/info` dispatcher. Native pending
metadata supplies the closed Normal/Critical/Bulk class through `httpReadClass`;
request JSON alone cannot promote a caller history lookup. The same orderStatus
endpoint is critical for amend sizing and bulk for a caller history query.
The host copies the class once at Prepare and keeps it through Bind. It is an
admission hint, with no signing, delivery or reconciliation authority, and never
becomes an HTTP header, URL parameter, body field or account identifier.

The total bucket remains 1,000 weight at 1,000 weight/minute. Noncritical work
can occupy 800 weight, leaving 200 for critical reads; critical work still obeys
the total. Explicit critical precedes reconciliation, then normal and bulk, with
FIFO inside each class. Reconciliation shares the reserve, while queued explicit
fence/amend reads retain first rank. Already physical work is not preempted.
Native refill is continuous at one weight per 60 ms, rather than a minute reset.
Candles and caller history are bulk; catalogue, fees and builder reads remain
normal. Account State/Orders, reconnect snapshots and Role/Mode/Spot prerequisites
use the closed reconciliation class. They keep the three-slot native bound;
explicit amend reads retain their independent slot behavior. Existing three-slot account limits, including foreground Orders,
remain in place. Native amend role/status weights are 60/2, matching the host.

Shared waiters retain 100 ms renewal and a 2 s lease. At most 128 are tracked,
with normal admission capped at 96 and bulk at 64 so they cannot fill every
critical queue slot. Weight-blocked preparations retain their FIFO position when
they reach dispatch. Cancelling one waiter removes only its own lease. The
shared 429 pause (Retry-After up to five minutes), generation check, established
IDB failure behavior and dispatch-only charging/refund rules remain mandatory.
No `/info` read bypasses a shared pause by changing its class.

Queued candles keep the original native 20 s deadline, shortened by an explicit
caller wait when smaller. History keeps its original tracked caller root and
mints a physical leg only when admitted, capped by that root. Expired queued
work fails visibly and cannot send later. Read class does not clear order fences,
change census epochs, replay writes or change the 334 ms mutation pacing.
An already issued routine read keeps its original census and owner; owed fresh
reconciliation retains the existing successor rules.

The WASM read-class export is present only with the Hyperliquid family feature;
older/other-family hosts use normal admission. Family/combined byte qualification
belongs to CI. Any required Hyperliquid cap change keeps Hyperliquid Spot equal.
PR-A's transport.http.timing stages continue to attribute prerequisite and write
queue/wire delay; this change adds no private telemetry or host rollout.

Cold-start correction after d020dff9ccf: the old shared Normal classification
held financial scopes below the 800 ceiling. The original four-engine fixture
reproduced at 62.8 s; changing only that ceiling to 1,000 gave 46.8 s. The fix
preserves both ceilings, classifies financial reads correctly, and delays only
startup ancillary issuance until pending financial/identity reads settle. A live
placement can promote the queued startup builder read without changing its
noncritical class or an issued deadline. Shared rank 0 is explicit critical;
rank 1 is financial reconciliation. No class changes HTTP material or authority.

The original fixture also lacked public data and eventually exercised the real
30 s public-silence reconnect instead of a healthy cold start. It now supplies
normal allMids pulses. With four engines and a genuine signed amend at 20 s,
financial scopes finish by 34.2 s, core by 0.2/8.2 s, and ancillary by 53.5 s.
Every State/Orders scope stays under 60 s; only the ancillary drain uses 64 s.

Metadata refresh scope is selected before changing shared maps. Normal same-DEX
metadata refreshes only that DEX; first discovery still refreshes its State and
Orders. A changed existing asset index, an asset-index collision, a coin-to-symbol
remap or foreign-scope identity/exclusion conservatively refreshes every known
DEX. Spot catalogue completion keeps its existing broad order refresh, including
spot/perp joins. Market table positions are not native asset ids: Store rebuilds
its table while account rows retain symbol identities. The existing discovery,
coverage, census epoch, revision and uncertainty checks remain authoritative.
