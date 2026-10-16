# Bitunix and Blofin contract review, first batch

Source base: `49fd1ff259`. This batch adds 70 selected contracts without
changing existing classifications or production Rust. It does not establish
the complete package accepted set, feature matrix, local-owner coverage or
WASM coverage.

The Bitunix compiler review selects 66 owners: 61 concrete function APIs and
five wire owners. Independent expectations cover numeric bits and fallbacks,
response and row selection precedence, order flags, endpoints, signature
rejection, hashing, virtual order IDs, four string-only enums and native
pending-position fields. All 66 execute maintained production-native probes;
none uses a classification-only receipt. The selected response Vec alias also
executes the actual decoder and conversion-trait probes.

Three Blofin aliases execute native trait probes under explicit nondecoder
classifications: `LifecycleFuture<'static, bool, String>`,
`PrivateStreamFuture<bool>` and `PrivateStreamStart<bool, String>`. The latter
two add exact bounded specifications to the existing generic-alias machinery.
Compiler-authenticated roots and invariant native type equality preserve the
complete future/result types. The lifecycle alias retains
`ServiceError<String>`. These checks do not claim to execute lifecycle callbacks
or cover other generic arguments and borrow durations.

Validation uses maintained `runProbes` with native/default configuration and
`jobs: 1`, sequentially: 66 owner receipts, one decoder-alias receipt and three
nondecoder-alias receipts pass. Two compiled controls modify only the copied
production source used by that runner:

- Replacing a finite parsed number with its fallback fails the named
  `value_helpers::parse_finite_number` native expectations; restoration passes.
- Removing only `strict_decode::object` from `PendingPosition::deserialize`
  admits the complete valid 20-field positional array and fails the named
  native observation. Its matching full object decodes successfully. Restoring
  the guard passes both assertions. Optional-field omission and present-null
  cases are also observed directly.

Both mutations compile and execute; compiler failures are rejected as control
evidence. Real production source hashes remain unchanged. The contract cases
and exact selected APIs are checked in; generated compiler reports, temporary
workspaces and execution logs are not.

The original review inventory has 199 Bitunix owners plus two aliases and
163 Blofin owners plus 13 aliases. This batch leaves 133 Bitunix owners and
its function-pointer alias, and all 163 Blofin owners plus ten aliases, for
subsequent review. Those counts describe this source snapshot rather than a
passing package ratchet.

## Subscription parser follow-up

Five further native/default contracts execute the concrete OHLCV and order-book
parser owners and their field/float/level helpers. Candle fixtures observe all
six native number fields, negative zero, ticker-before-callback order, absent
tickers and callback errors. Book fixtures inspect both native sides, independent
cumulative amounts, original Rc identity and partial state when asks refuse
after bids have been replaced. Failure callbacks retain the completed book.

All five production-native receipts pass with jobs1. Two independently compiled
copied-production mutations replace candle open with close and accumulate book
prices instead of amounts. Each fails its named owner; exact restoration passes
both owners. These are runtime assertion failures, not compiler failures, and
the real production source bytes are unchanged. This follow-up leaves 128 of
the original Bitunix owners for subsequent review.

## Concrete helper and error-conversion follow-up

Another 47 Bitunix owners execute native/default probes: 46 concrete functions
and the three inventoried conversions on `adapter::Fault`. Their expectations
observe numeric precision and rounding, own-property order and Undefined/null
identity, first/last table precedence, historical UTF-16 selectors, REST field
selection, retry short-circuit order, response-body phases, TP/SL precedence and
native error class/message/response metadata. All 47 owner receipts pass.

Compiled copied-production controls remove inverse-table reverse precedence and
invert account-mode selection. Both fail their named native owner and pass
after exact restoration. Production source bytes remain unchanged. The query
fixture was also checked against the source's actual `qs` dependency: repeated
array keys and null's empty assignment remain observable.

Nine Blofin callback/factory aliases separately pass their actual native
Deserialize/conversion trait probes under specific nondecoder classifications.
Eight use the existing fixed `serde_json::Value` error-parameter callable policy;
`CommonHttpFactory` is concrete. Exact compiler targets and authenticated roots
are retained. These are capability classifications, without callback execution,
other generic arguments or permission for neighboring host methods.

This follow-up has 56 receipts: 47 runtime owners and nine explicit alias
classifications. On the original compiler inventory, 81 Bitunix owners and its
retry function-pointer alias remain, alongside all 163 Blofin owners and its
API-factory function-pointer alias. The existing Future specifications and every
previous contract remain unchanged.

## Historical result mappers

The trade-to-income, closed-position and historical-order functions now have
three further native/default runtime receipts. Expectations inspect canonical
enum variants, all mapped numeric bits, IDs, account/asset/provenance fields,
optional presence and source null/empty guards. The closed-position cases keep
closed quantity distinct from peak quantity, with separately chosen close prices.
The trade cases distinguish commission from rebate and verify PnL-before-fee
ordering. Historical orders retain zero-as-undefined price/time fields.

All three actual production owners pass. Three compiled copied-source controls
invert fee sign, replace notional's close price with quantity, and invert the
market-order test. Each fails its named owner; exact restoration passes all
three. Real production source bytes are unchanged. This leaves 78 original
Bitunix owners, alongside the previously listed pointer and Blofin scope.
