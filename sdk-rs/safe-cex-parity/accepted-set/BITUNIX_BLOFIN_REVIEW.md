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
