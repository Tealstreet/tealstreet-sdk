# Hyperliquid initial private-stream recovery

Hyperliquid and HyperliquidSpot source engines require the first native account
snapshot for each subscribed clearinghouse scope and spot holdings, within 30
seconds after their private subscriptions are released. Public prices, pongs,
subscription acknowledgements and successful REST reads do not satisfy this
requirement. A native snapshot received before the release continuation also
counts. Once these initial snapshots arrive, quiet account events need no
recurring private-data heartbeat.

A missed deadline retires the current managed socket epoch and reconnects through
the existing native backoff, replaying current demand and refreshing account
reads. Retirement detaches the old socket before retry; neither the engine nor
the JS host waits for a browser socket stuck in CLOSING to deliver its close
callback. Retired frames cannot satisfy the replacement's snapshot watch or
publish account state. A delayed hidden-tab timer or message checks the original
deadline before accepting late private data.

SDK, web and hosted callers retain the existing worker/frame protocol. A retained
frame-fed page engine owns no source socket, and `register reused` describes that
page engine; it does not show that the local source worker was reused. Local
reload cleanup and re-registration recreate the source worker independently.
No NX change, write replay, caller rollout or deletion prerequisite is added.
