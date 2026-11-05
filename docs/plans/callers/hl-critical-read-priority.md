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
the total. Critical precedes normal, then bulk, with FIFO inside each class.
Native refill is continuous at one weight per 60 ms, rather than a minute reset.
Candles and caller history are bulk; startup/catalogue and routine account work
remain normal. Existing three-slot account limits, including foreground Orders,
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
