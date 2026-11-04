# Hyperliquid initial private-stream recovery

Hyperliquid source engines require the first native core clearinghouse snapshot;
HyperliquidSpot requires its first spot snapshot. HIP-3 and spot holdings on a
perp account are best-effort and never gate recovery. The first deadline is 30
seconds after private subscriptions are released. Public prices, pongs,
subscription acknowledgements and successful REST reads do not satisfy it.
A native snapshot received before the release continuation also counts. Once
the required initial snapshot arrives, quiet account events need no recurring
private-data heartbeat.

Consecutive bootstrap misses grow the next deadline to 60, 120, 240 and then 300
seconds, capped at 300 seconds. This venue-owned streak resets only when the
required snapshot completes a watch; pongs, public data, opens and socket
retirement cannot reset it. One warning is emitted per failed-bootstrap episode.
Scopes added later on the same epoch are not watched; a replacement epoch arms
a new initial-snapshot watch.

A public read-only probe on 2026-11-04 observed initial empty core, all 10
then-listed HIP-3 scopes and spot snapshots for an unused public address within
441 ms, without credentials. Optional scopes remain best-effort even though
this probe succeeded; their presence is not a recovery requirement for perps.

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
