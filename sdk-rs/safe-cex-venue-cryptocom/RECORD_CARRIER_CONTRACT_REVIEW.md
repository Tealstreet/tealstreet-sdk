# Crypto.com record and carrier contracts

This slice reviews six existing native/default concrete wire records against
cryptocom.types.ts: Instrument (17 scalar fields), Ticker (11), Candlestick (6),
PositionBalance (9), Error (2) and the inline AccountIdentity (1). It adds no
production decoder behavior or manifest status change.

Independently authored inputs distinguish all 46 declared scalar fields. Native
observations read each field directly, compare f64 bits including signed zero,
exercise Instrument's four booleans one at a time and preserve unknown nested
values as native JSON kinds and number bits. The canonical observations also
inspect private captured input_keys and call own_keys to check retained order
and presence. Tests execute all six ordinary/numeric text and owned/borrowed
Value decoding paths. Complete valid arrays are in each private companion's
Serde declaration order, with skipped metadata fields excluded.

Each concrete decoder already rejects arrays at two stages: wire_record::capture
requires an object, then the private remote companion uses strict_decode::object.
The maintained array controls intentionally bypass BOTH stages by delegating
the concrete owner directly to its same original remote companion. Removing
only the inner guard would not widen the actual concrete frontdoor. A control
must compile, fail the named complete-array rejection assertion at runtime and
pass after exact byte restoration. This is a compound boundary-bypass proof,
not evidence that either individual guard alone admits arrays.

The six specific construction reviews cover derived Clone on ordinary owned
scalar/JSON data and private captured-key metadata. Native observations compare
actual clones; the tests additionally modify cloned nested extras and check
that the original remains unchanged. These carriers have no one-use validation
state or registration authority. This does not widen their raw input domains.

Private remote companions return the concrete owners and have no independent
Deserialize implementation. Ordinary private Wire decoders keep their own
obligations. Existing shared string, number and owned-JSON boundaries apply;
these tests do not establish arbitrary live JavaScript object identity.

Five existing carrier enum APIs are reviewed as actual decoders: the websocket
unions (valuation channel, side, reference-price type, position type and isolation
type). Their 18 tags have independently authored direct native variant/ordinal observations.
PayloadValue APIs also reject non-string primitive kinds and valid-tag object
maps. They receive observed decoder contracts instead of typed-construction
waivers.
Each owner has a compiling literal identity/refusal mutation control.

From sdk-rs, run:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_scalar_record_shapes
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_carrier_enum_shapes
python3 safe-cex-venue-cryptocom/scripts/scalar-record-controls.py
python3 safe-cex-venue-cryptocom/scripts/carrier-enum-controls.py
node safe-cex-venue-cryptocom/scripts/record-carrier-contracts.mjs
```

The maintained selector fixes these eleven compiler identities, performs fresh
rustdoc, loads the per-crate contract shard and executes canonical native
production probes. It separately matches the six selected compiler Clone
capabilities with loadPolicy('construction') and their specific Crypto.com
shard reviews. These checks do not replace the actual Clone value observations.

After integrating master 69fe03c566 and policy split 315, the selected gate passed
14 native decoder owners with zero failures/classification-only receipts and
matched all six selected compiler Clone capabilities with their reviewed rows.
All 14 compiled mutation/restoration controls passed. The integrated scoped suite
passed 128 with zero failures/two existing ignored tests; host/WASM all-target
Clippy, formatting and parity 0 (7459 rows/17204 public Rust items) passed.
The deleted flat policy tables remain absent. This slice covers only fourteen
of the original 133 native/default owners and six of fifteen construction reviews. Other owners,
aliases, construction entries, newer discovery, locals, features and WASM
contract execution remain separate obligations. No full-package completion is
claimed.

The test-only `account_envelopes.rs` module had no production caller and was
removed with its three carrier owners (wave2-vacuous-tests). The validation
paragraphs above record the larger batch as it passed before that removal; the
selector, control runner and contract shard now hold the eleven owners.
