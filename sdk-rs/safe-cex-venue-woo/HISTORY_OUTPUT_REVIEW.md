# Historical output and option boundaries

The remaining history leads were reachable through the configured exchange.
`woo.exchange.ts:1405–1582` stringifies truthy order IDs, numerically coerces
limits and timestamp operands, and ignores unused option properties. The native
member previously decoded the entire option object as a strict shared record,
rejecting numeric IDs, string/array/null limits, null bounds and unread UTF16 data
before any signed request. Its adapter now reads only source-consumed properties,
uses the installed symbol normalizer and native String/Number operations, then
passes the resulting typed operands to the existing single history pipeline.
Public-only guards still precede option reads; internal trades still require a
truthy symbol. Shared public Rust option types are unchanged.

The selected `commissionAsset` had another batch-loss boundary: source copies
an arbitrary truthy value, while the shared Execution/HistoricalTrade fields
remain optional Strings. The old conversion rejected every selected fill if one
asset was an object. Per shared decision 3, one admission pass after source
filter/sort/slice now reports the malformed row's id, field and original value,
omits that row, and keeps good siblings. It neither coerces non-string assets nor
backfills the limit with previously unselected rows. Reporter errors propagate.
JSON spelling occurs only for the visible refusal diagnostic, after selection.

Typed String fields use the approved UTF8 addendum: isolated units become U+FFFD
at the typed symbol/id/asset boundary. Raw Input responses remain lossless. Side
and position-side vocabulary still follows the original recognized-tag guards;
an unknown surrogate-valued side skips its row, and unknown position side keeps
the source hedge/both fallback. A selected numeric surrogate string still becomes
NaN via the shared retained primitive and fails only the source validity guard.

The 28 maintained-source/signed-native profiles cover both callers, numeric order
IDs, falsy IDs, string/null/array limits, string/null/nonfinite bounds, unread UTF16
options, good/bad/good asset rows, typed surrogate text and unknown side tags. The
observer checks every output own key and binary64 number before JSON, one visible
asset refusal with its original value, one signed HTTP request and its signature.
The source result retains the malformed object; the native observer explicitly
applies the approved row-refusal policy rather than changing the source oracle.
Only typed text is observed with the approved U+FFFD replacement.

Three named real-binding regressions compile and fail with current master's
transaction/parser, historical-output and member-adapter production files. Exact
fixed bytes are restored after the replay; the new history suite then passes.
The prior 64 retained-page and 14 row-admission profiles remain intact. The
original 48 caller and 18 boundary source profiles keep their inputs and source
results: the selected-asset cases now assert two surviving good rows and one
visible refusal, replacing their former whole-result error expectations. Native
boundary totals are 42 outputs, two row refusals, 18 requests and four reports.

The final class reconciliation is recorded in EXCHANGE_CLASS_EVIDENCE.md. It
uses actual maintained WOOXExchange/DefaultStore construction and native signed
cache operations. These findings close assigned ledger rows 19, 20 and 60;
strict native required arguments, owned snapshots and explicit host capabilities
retain their category-wide authorizations. This changes no shared model or TS
production behavior and adds no fallback or parallel history path.
