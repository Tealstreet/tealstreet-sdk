# Hyperliquid fast book depth

Fast depth defaults OFF. SDK callers opt in per exchange with
`hyperliquidFastBook: true`; registry/native construction maps it to
`Options.fast_book`. No global flag or NX setting is introduced. Standalone spot
keeps its existing five precision streams even when the option is supplied.
Shared public-feed identity includes the effective option so off callers cannot
silently acquire an opted-in source. Existing refcounts and epoch custody apply.

Opted-in perpetual books add one fast `l2Book` topic alongside slow depth/BBO.
Live anonymous BTC capture on 2026-11-06 found 20/20 slow rows without `fast`
and 5/5 fast rows with `fast:true`. Covered deep rows stay removed until a new
slow snapshot; a fast side shorter than five rows exhausts its old tail.
Equal-time fast data wins; retired socket data is inert. Last-price behavior
and private topic selection remain unchanged.

New book/candle demand above 1,000 topics is refused visibly and rolled back
before proposal. Accepted references and the feed remain live; an explicit
close/retry/new Open replays exactly the accepted set. The encoder also refuses
an oversized projection before release. This per-connection guard cannot enforce
the venue's 1,000-subscription PER-IP cap across accounts/tabs/other clients.

For M core markets, D HIP-3 scopes, B books and C candle streams, Direct keyed
feeds cost 9+M+5D+(2+F)B+C (+1 when spotState is needed); public-only costs
2+(2+F)B+C. F=0 by default, F=1 when opted in. With illustrative M=235/D=0:

| Topology | Default/before | Fast opted in |
| --- | ---: | ---: |
| Four keyed engines, 20 books each | 1,136 | 1,216 |
| Three keyed engines, 20 books and 20 candles each, plus one public engine with 20 books | 954 | 1,034 |

The first topology already exceeds the per-IP cap. Keep fast depth off until
aggregate demand is budgeted; subscription diet and public-feed sharing follow
separately. No new engine API or deletion prerequisite is introduced.

References: https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/websocket/subscriptions
and https://hyperliquid.gitbook.io/hyperliquid-docs/for-developers/api/rate-limits-and-user-limits.
