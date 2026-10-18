# Crypto.com nested wire contracts

This slice reviews seven existing concrete records: Trade, AccountsResponse,
BalanceSummary, BookData, Book, TradeFill and Order. Their 69 declared fields
include nested AccountIdentity and PositionBalance records, three-string book
tuples, finite enum values and optional fields. Trade.dataTime is explicitly
associated with the native data_time field. Production decoder behavior and
manifest statuses are unchanged.

Independently authored expectations inspect native fields, numeric bits including
signed zero, enum variants/ordinals, captured input_keys and own_keys, retained
opaque extras, collection lengths and item order. Six ordinary/numeric text and
owned/borrowed Value paths execute each concrete owner. Positive cases include
reversed input key order, zero/one/two collection entries, distinct bid/ask tuple
values, missing optionals, and present empty strings, false, zero and arrays.
Present null is refused by non-null optional field decoders. Actual Clone values
are compared with originals; tests also mutate cloned extras independently.

Complete valid arrays use actual Serde declaration order, including present
optional values and excluding skipped metadata. Seven controls delegate the
concrete owner directly to its same private remote companion, bypassing BOTH
existing object boundaries: capture and strict_decode::object. Removing only the
inner guard cannot widen this concrete frontdoor. Five collection sites each
have empty/subset controls after full input consumption, so trailing input cannot
supply a false refusal. Each control must compile, fail the named native assertion
with exit 101, then pass after exact byte restoration.

The two private present-value functions receive direct decoder contracts and
unit witnesses rather than parent-record exemptions. String decoding preserves
empty/Unicode text and refuses other JSON kinds. Generic decoding executes
String, f64, bool, Vec<String>, RefPriceType, OrderStatus and Option<String> with
an actual JSON text Deserializer. Some(None) is distinguished from outer None.
Two compiling controls erase successful presence and require the direct native
helper assertion to fail before restoration. These instantiations do not claim
universal behavior for arbitrary T or custom Deserializer implementations.

Seven construction reviews cover derived Clone of these owned data carriers and
their captured-key metadata. They carry no one-use validation state or authority.
The closed selector uses fresh native/default rustdoc, split contract storage,
loadPolicy('construction'), exact selected compiler traits and canonical native
production probes. Runtime observations remain independent of compiler shape.
Private remote companions return their concrete owner and have no independent
Deserialize implementation; ordinary private Wire decoders remain separately owed.

Run sequentially from sdk-rs:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_nested_record_shapes
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib wire::accepted_present_helpers
python3 safe-cex-venue-cryptocom/scripts/nested-record-controls.py
python3 safe-cex-venue-cryptocom/scripts/nested-cardinality-controls.py
python3 safe-cex-venue-cryptocom/scripts/present-helper-controls.py
node safe-cex-venue-cryptocom/scripts/nested-wire-contracts.mjs
```

Selected validation passed: 14 external native tests, two direct private-helper
tests, all 19 compiled mutation/restoration controls, and nine canonical native
decoder results with zero failures or classification-only receipts. All seven
selected compiler Clone capabilities match their specific construction reviews.
After merging master f3691e05a6, the integrated scoped suite passed 144 tests
with zero failures and two existing ignored tests. Strict host/WASM all-target
Clippy, formatting, parity (7459 rows/17204 public Rust items, zero findings),
and the fresh nine-owner/seven-construction gate passed. This is nine original
native/default census owners and seven specific construction reviews. Other
owners, aliases, newer discovery,
locals, features and WASM contract execution remain separate obligations.
