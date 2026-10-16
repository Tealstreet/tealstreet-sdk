# Quote-read slice precision audit

Current production adds one paired precision construction in quotes::fetch_markets:
amount <- minOrderSizeIncrement; price <- priceIncrement. There are still zero
implemented step-adjust/rounding calls. The historical zero-consumer census below
applies only to its listed revisions.

ALREADY COVERED for both new fields: changing the amount assignment to the price
increment compiled and failed QUOTES-NATIVE fetchMarkets; changing the price
assignment to the amount increment independently compiled and failed that named
assertion. Both restored and passed. The original live fixtures use amount .1 /
price .01 and amount 1 / price .25, and no fixture values changed for these controls.
This proves those two assignments are distinguishable; it does not claim future
order quantity, limitPrice or stopPrice rounding is swap-proof. Those three pending
write sites still require fractional distinct-step native payload controls.

---

# Price/amount precision swap audit

Audited commits:

- Pinned PR 7257: `42da273dc319fbfd2bb2a3daeb3e0295463352a2`.
- Local portfolio-read slice: `17b875e9bf94884289d0a14e28f2bc768eddfcf8`.

Method: enumerate committed production modules with `git ls-tree`, read every
function body, and trace shared-helper calls for precision forwarding. Independent
sub-agent review repeated this procedure. Scope was 4 files / 787 lines pinned,
5 files / 873 lines local. Search was a locator, not the zero's evidence.

Both revisions have **0 price/amount step-adjustment sites**, **0 paired precision
consumers**, and **0 forwarding calls carrying those precisions to shared helpers**.
The instrument codecs retain priceIncrement and minOrderSizeIncrement; they do
not use them to format outgoing values. Portfolio formatters copy native price
and quantity and calculate PnL/margin; no precision adjustment occurs. The local
read wrappers only call these formatters and read stored fallbacks.

The unported TypeScript still has the susceptible shape:

| Source row | Precision work | Current manifest status |
| --- | --- | --- |
| fetchMarkets | amount <- minOrderSizeIncrement; price <- priceIncrement | pending |
| placeOrder | one paired consumer; amount/pAmount, price/pPrice, stopPrice/pPrice: three adjust calls | pending |
| updateOrder | passes replacement price/amount into placeOrder | pending |

Therefore **no compiled two-axis site exists to swap yet**. No precision mutation
was run, and no swap-proof claim applies to future write formatters or shared
crates. When placeOrder is ported, its corpus must execute actual TypeScript with
fractional values whose price/amount steps produce DIFFERENT results, across
several step pairs, both sides, limit and stop-market variants (including trigger,
stopLoss, and takeProfit selection). Observe native quantity, limitPrice and
stopPrice separately; prove every production precision swap compiles, fails its
named assertion at exit 101, and passes after restoration.

No manifest row status changed for this audit. The reviewed remote remains pinned.

Current quote-read slice supersedes the historical pending fetchMarkets entry:
1 paired precision producer, 0 adjustment consumers, 2 compiling swap controls
failing QUOTES-NATIVE fetchMarkets then restoring/pass. Only the named historical
revisions had zero paired sites and no precision mutation run.
