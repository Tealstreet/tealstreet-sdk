# Socket book comparator review

The constructed Crypto.com public socket calls `sort_levels` for asks and bids
from the actual `listenOrderBook` parser. Source `utils/orderbook.ts` compares
prices by subtraction. The old binding cached prices and used `f64::total_cmp`,
which moves signed-zero ties and NaN prices instead of following the stable
subtraction comparator. Amount accumulation then assigns totals to different
levels.

`exchange_binding::socket_book_{bids,asks}_match_source_subtraction_sort` sends
real JSON frames through the production socket, parser, sorter and typed callback.
Only terminal HTTP/socket/time capabilities are scripted. Each side is exercised
independently, with the other side empty. The source oracle executes the maintained
listener and actual order-book utilities on CI Node 24.20.0 / V8 13.6. Thirteen
profiles include signed zero, NaN, infinities and lengths 22 through 257; all price,
amount and cumulative-total bits are compared, canonicalizing NaN only.

The repair uses the canonical V8 TimSort and reads/coerces both price properties
in source comparator order on each comparison. No cached key, total-order
substitute or new sorter is introduced. The callback still receives the original
level objects and source amount totals. The enclosing `open_window` throttle
helper is unchanged; only its whole-file source fingerprint is refreshed.

The crate's other Rust comparator sites order UTF-16 signing keys lexicographically.
Those comparisons are total and require no migration.
