# Crypto.com runtime helper roles

This slice reviews eight original native/default helpers individually. They
operate on existing generic native carriers through explicitly supplied host
operations. They do not admit raw JSON, validate a wire record, decode a finite
literal union, or construct a validation token. Downstream decoders and the
functions they invoke retain their own contracts.

| Helper | Reviewed role | Direct native evidence |
| --- | --- | --- |
| get | Property-operation forwarding | Getter/result and thrown-value identity |
| optional | Optional-chain control flow | Null/undefined skip lookup; existing objects forward lookup/errors |
| method | Method dispatch | Callee lookup before argument thunk/invocation, original receiver/argument/result identities, abrupt completion |
| own | Receiver-cloning dispatch | Actual Js receiver clone retains identity and delegates the same call/error order |
| global | Global-call dispatch | Global lookup before arguments; undefined receiver; argument/result/error identity |
| text | Message rendering | Prefix/coercion/suffix UTF16 units, including lone surrogates, and coercion errors |
| loaded | Outbound loaded-flag patch | Existing value references retained, selected flags true, original property bag unchanged |
| update | Outbound Store write | Getter/fields/object/invoke order, original Store receiver and field value, abrupt completion |

Tests call each actual private helper directly with shared js_runtime::Js and
Thrown values. SocketOps supplies the used primitive operations; unused host
capabilities panic so unexpected dispatch cannot silently succeed. These
concrete witnesses do not establish behavior for every arbitrary host or Clone
implementation. In particular, the own witness executes Js cloning and does
not claim that every generic V clone preserves reference identity.

Each helper has a compiling corruption control with a named runtime assertion
failure, exact source-byte restoration and passing rerun. The closed native
selector uses fresh rustdoc, exact compiler APIs and the per-crate contract
shard. Its eight non-decoder classification receipts are separate from the
actual runtime unit witnesses and mutation controls; compiler metadata alone
is not runtime evidence.

Run sequentially from sdk-rs:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib runtime::accepted_runtime_helpers
python3 safe-cex-venue-cryptocom/scripts/runtime-helper-controls.py
node safe-cex-venue-cryptocom/scripts/runtime-helper-contracts.mjs
```

Selected validation passed: eight direct native unit tests, all eight compiling
mutation/restoration controls and eight fresh API classification receipts with
zero failed owners. The receipts are classification-only; actual runtime
evidence comes from the separate tests and controls. Integrated validation
against master c442617880 passed: 152 scoped tests, zero failures and two
existing ignored tests; strict host and WASM all-target Clippy; formatting;
parity (7459 rows, 17204 public items, zero findings); and the final fresh
eight-owner classification gate. No production behavior or manifest
classification is changed. Scope is these eight original native/default owners. Other owners,
aliases, construction entries, newer discovery, locals, features and WASM
contract execution remain separate obligations.
