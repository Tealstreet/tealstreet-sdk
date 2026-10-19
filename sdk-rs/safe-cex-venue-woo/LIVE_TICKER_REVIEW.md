# Live WOO ticker audit

Assigned rows 49, 50 and 51 are ACCEPT with the existing decision-3 boundary.
This review changes no production mapper or shared type. The manifest keeps these
rows divergent because TS copies arbitrary values into typed ticker fields.

## Source and runtime

The maintained `woo.ws-public.ts:108–156` selects only `PERP_` symbols, normalizes
them, looks up the existing ticker and updates these fields in source order:

| Topic | Source to native field |
| --- | --- |
| tickers | close to last; amount to quoteVolume; volume to volume |
| bbos | bid to bid; ask to ask |
| markprices | price to mark |

The constructed exchange installs all three handlers through its real public
socket class. `public_socket.rs` reads canonical retained values without a whole
JSON projection. Numeric overflow and signed zero survive parsing and Store
mutation; unused metadata can contain lone surrogates. Unknown/non-perpetual
symbols do not write. Normalization precedes UTF8 identifier admission.

Decision 3 in `shared-type-decisions-final.md` explicitly keeps Ticker fields
strict: a bad row is reported with its symbol, field and original value and is
omitted as a whole while valid siblings commit. A later source lookup failure
retains precedence. This acceptance does not claim source equivalence for values
that cannot enter the typed Ticker, nor authorize a global coercion.

## Native and source evidence

`binding_streams::production_ticker_coercion_matches_constructed_typescript`
constructs the maintained TS exchange and DefaultStore on every run, drives the
native exchange through actual socket frames and compares all eleven numeric
fields by binary64 bits. Its 72 packed three-row cases retain 54 visible bad-row
refusals and both valid siblings. The independent source body is not rewritten.

`public_socket::write_tests::public_writer_matches_numeric_bits_and_notification_failure`
uses 87 maintained-handler/DefaultStore cases. It observes committed first-row
writes before subscriber failure, skipped later subscribers/rows, original cause
identity and Store-local subscriber ids. `public-writes-oracle.cjs --check`
independently verifies the complete current source fixture.

The existing native retained-frame tests cover all three numeric-row surrogate
refusals, ignored metadata, overflow/signed zero and normalization before admission.
An additional unread-metadata witness checks all three real socket topics, including
native infinity and signed-zero bits rather than JSON-rendered values.

Three independent compiled controls change the actual ticker, BBO and mark field
mapping in turn. Each must fail the constructed-source numeric assertion; exact
production bytes are restored before the final native pass. These are acceptance
witness controls: current master behavior already passes, so this is not claimed
as a newly fixed production bug.
