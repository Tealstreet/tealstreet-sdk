# Ticker parser and composed read slice

Base: fetched integration `6b045b13e08940bca0eb23d233d86d28d274e2e8`.
Python 3.11 tomllib: 14 ported / 9 divergent / 56 pending, 79 total.
`parseTickers` and `fetchTickers` are CHOSEN divergent with shared JSON/UTF-8,
typed Store and snapshot-identity boundaries stated in their manifest notes.
API construction remains pending. No capability finding is re-derived.

One Node process executes 41 actual parser cases and six actual composed reads.
Source dependencies and AST initializers execute directly. Native expectations:
55 tickers, 715 field comparisons, 18 errors, six reads, three emissions and two
completed rejection-identity checks. Capture records Node v26.7.0 / major26 /
arm64 / Darwin; NaN alone is canonicalized and signed zero remains exact.
The five original oracle files are byte-identical to the base.

`tests/ticker_parser_controls.py` recorded 11 named failures and 11 restored
passes. Controls cover bid/ask selection, percentage divisor, quote-volume input,
original-case suffix filtering, consumed empty/subset and extra native outputs,
wrapper input loss, empty/subset source corpus and disabled identity assertions.
The reviewer found the identity assertion gate had no execution count; its two
checks are now counted, and disabling the gate fails the complete comparison
census. This is a repaired test gap, not a production defect.

`tests/ticker_parser_layer_controls.py` recorded four named failures and four
restored passes. With both native length guards disabled, consumed empty/subset
output fails `PHEMEX-PARSER fixed native output coverage`. With the source
per-case length guard bypassed, empty/subset expected output fails
`PHEMEX-PARSER expected/native output count`. These are post-consumption output
controls, not a claim about a newly implemented serde decoder.

The source field inventory is fixed per output; native fields are checked before
serialization. The existing 78-control audit is preserved. Its additional-shape
census was zero assertion-bearing gated closures and two unbounded runner
selections (both repaired), plus one adjacent gated assertion block (repaired).
No price/amount precision adjustment is implemented by these two read methods;
this is a negative site census, not precision-swap immunity.

Exact-SHA readiness gates and the review verdict are reported with the pushed SHA;
this file records executed mutation evidence and does not predict future gates.
