# KuCoin socket recovery

Every detail, snapshot-shard, and private feed has its own engine `Supervisor`.
Reconnects back off exponentially with deterministic jitter and POST a fresh
role-specific bullet token. A 10-second acquisition/connect deadline retires
hung requests; per-feed generations reject late token replies. Failed token
requests no longer burst immediate retries.

The bullet response still selects the endpoint and heartbeat timings. Native
welcome and mandatory-pong deadlines remain independent of general data traffic:
an early pong cannot replace welcome, and ordinary data cannot satisfy an
outstanding ping. The Supervisor supplies the negotiated ping cadence; only
welcome marks a session authenticated and starts subscriptions. Futures REST
account reconciliation starts at launch and does not wait for the private socket.

KuCoin Spot HF uses the same supervision policy for its public and private
feeds. Each retry POSTs a fresh role-specific bullet token under a single-flight
acquisition with a generation fence. Token acquisition and transport opening
have separate ten-second deadlines; only a native welcome after opening marks
readiness. The bullet server's validated heartbeat timings drive the welcome
and mandatory-pong deadlines. Ordinary data and early pongs cannot satisfy
those deadlines. Late retired token responses/failures and socket frames are
inert, and welcome restores current ticker/book/candle or private account topics.

Private GET reads in both markets recover once from the native `400002`
timestamp refusal. They fetch public `/api/v1/timestamp` from their own REST
origin, calibrate the engine's protocol clock at receipt, then sign the exact
same path and query with the original selected identity. Normal startup adds no
clock request. The probe has a five-second cap; it and the resend remain under
the original read root and caller deadline. Invalid time, expiry, transport
failure, or a second timestamp refusal ends that attempt. Existing periodic
account reconciliation can still start a new read on its normal schedule.

History and spot lookup continuations retain their original command root across
pages and retries. Background snapshots use a bounded root for each physical
page. Public reads stay unsigned when no identity was selected. Trading jobs
and POST socket-token requests do not enter this recovery path; writes are
never replayed on `400002`.
