# Hyperliquid fast book depth

SDK native HL perpetual sockets add one `l2Book {fast:true}` topic per distinct
book alongside slow depth and BBO. NX/server callers need no API changes and
must retain existing refcounts, public-feed ownership and epoch lifecycle.
Standalone spot subscriptions are unchanged. This change does not alter last
price or private subscription selection.

`data.fast` explicitly distinguishes the shallow response; live unauthenticated
BTC capture on 2026-11-06 observed slow 20/20 responses without the field and
fast 5/5 responses with the field. Fast top coverage overrides old depth and
removed baseline rows cannot reappear on a retreat. A full newer slow snapshot
refreshes deep rows. A fast side shorter than five rows proves exhaustion and
clears its old tail. Equal-time fast data wins; retired socket data is inert.

Every perp book now costs three subscriptions. The native projection refuses
more than 1,000 topics before release and a capacity-refused book addition
rolls back only that new book. This cannot enforce the venue's shared per-IP
limit across engines, tabs or other clients; callers must budget their aggregate.
The keyed count is 9 + core markets + 5*HIP3 scopes + 3*books + candles, with
one more for spotState when enabled. Public-only uses 2 + 3*books + candles.
No legacy socket, new engine API or deletion prerequisite is introduced.

References: Hyperliquid's official websocket subscriptions (`l2Book.fast`, five
versus twenty levels) and rate-limits/user-limits (1,000 subscriptions per IP).
