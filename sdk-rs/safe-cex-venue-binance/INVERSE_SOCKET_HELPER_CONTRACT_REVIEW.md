# Inverse socket helper contracts

The compiler inventory discovers eleven additional raw-string function owners:
private `call`, `flag`, `global`, `optional`, `send`, `timeout`, and public `call`,
`flag`, `global`, `subscription`, `tickers`. They all have executable decoder
contracts, rather than exemptions or classification-only receipts. The separate
seventeen-owner REST/type cohort has its own runner and observers.

`node sdk-rs/safe-cex-venue-binance/scripts/inverse-socket-helper-contracts.mjs`
discovers the actual package with pinned rustdoc in `target/accepted-inventory`,
then uses the maintained accepted-set instrumentation and production runner.
The eleven helpers execute unchanged in their owning modules. The 79 finite
cases check missing/undefined/null/inherited values, accessor failures, receiver
and argument identities, original thrown objects, signed zero/NaN bits, UTF16
code units, serialization, timer guards/cancellation, subscription numbering and
ticker parsing/quote fallback. Numbers and native handles are observed before
finite JSON reporting. The compiled missing-classification control names all
eleven owners; the successful run has eleven runtime receipts and no exemptions.

The observer reuses the primitive socket runtime and actual inverse constructor.
Method/import/Store capability probes install trace functions at their named
boundary to check receivers, arguments, writes and error propagation. They do
not replace any helper under review, and do not establish exhaustive equivalence
for arbitrary executable getters or object graphs. The existing
`inverse_socket_binding` target separately exercises actual assembled sockets,
HTTP clients and DefaultStore; its maintained whole-class source oracle covers
eleven groups including the recorded UTF8 frame refusal and frozen book callback
profiles. Those explicit profiles are retained.

`python3 sdk-rs/safe-cex-venue-binance/scripts/inverse-socket-helper-controls.py`
compiles one isolated fault per helper, requires named runtime failure (compiler
errors do not count), restores exact production bytes, and requires the same
owner to pass. The script writes measured receipts under `/tmp`; it must finish
successfully before claiming those controls complete.
