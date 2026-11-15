# Kraken Futures position notional

PF position sizes already use base-coin units, so their quote notional is the
absolute native size multiplied by a finite positive mark. The position row's
mark takes precedence over the public ticker. Missing or unusable marks leave
notional unknown while retaining the position and its coin size. Unknown is NaN
in the native frame and null in JSON.

REST open-position rows omit marks. A same-size, finite positive websocket
valuation remains usable; zero, invalid or unknown cached values cannot prevent
recovery when a valid ticker becomes available. A changed size cannot inherit the
old valuation.

Inverse quote value is contract count multiplied by a finite positive quote
contract value, independent of mark. Discovery remains limited to PF products;
PI/FI metadata is seeded only in tests to check the distinct units.

Sources: [open positions](https://docs.kraken.com/api-reference/account-information/get-open-positions),
[position stream](https://docs.kraken.com/exchange/api-reference/futures-websocket/open_position),
[linear contract specifications](https://support.mtf.kraken.com/hc/en-us/articles/25499987607837-Linear-Contract-Specifications),
and [inverse versus linear examples](https://support.kraken.com/au/articles/360022829971-what-are-derivatives-other-frequently-asked-questions).

The shared Position contract requires `notional: number | null`; its Rust
counterpart uses `Option<f64>`. JSON null survives typed Store and numeric ingress.
SDK exposure totals and leverage propagate unknown, combined-balance reads and
notional-based sizing tokens refuse it, and CLI valuation displays use `—`.
Position/chaser presence follows quantity. Inverse stop planners refuse unknown
quote capacity before HTTP; a known independent native size remains usable.

Hosted Rust CLI projections follow the same unknown-valuation display policy.
Their close and percentage sizing call the shared native-size helper: a known
independent native quantity remains usable; conflicting quantities refuse.
