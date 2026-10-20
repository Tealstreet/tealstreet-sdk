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
welcome marks a session authenticated and starts subscriptions/reconciliation.
