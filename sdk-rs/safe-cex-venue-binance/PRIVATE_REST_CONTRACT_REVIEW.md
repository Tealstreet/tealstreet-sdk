# Private REST native contracts

This cohort covers the seven function owners assigned from the Binance receipt
census. `endpoint` selects generated static paths from internal venue/literal
keys and has an explicit non-decoder review. `keyed`, `map`, `order_type`,
`pm_rows` and `find_futures_order_by_client_id` have executed production contracts.
The recursive `json_data` helper is removed, with no remaining caller or adapter.
The three `Snapshot*` callable aliases remain a separate cohort.

`keyed` now uses the canonical native `in` operation for the exact/snake/camel
alias walk. It returns the original property, including own undefined, without
traversing unrelated array elements. The map contract checks native values,
initial-length behavior during appends and retained element identity. The lookup
contract checks cached identity, primary/private dispatch, definitive not-found,
missing markets, remaining bits and original response info.

The shared native array validator reads only the top-level shape and the source
`msg || message || error` / `code` fields. Existing JSON APIs delegate to it and
still return a slice borrowed from their original input. Its direct contract
also observes the native shape and message helpers, including UTF16 keys/text,
own undefined and nonfinite Number kind. These scalar runtime helpers return
native UTF16 strings; they are not separate raw-JSON admission APIs. The old
UTF8-only diagnostic constructor is replaced by the native constructor, with
all three existing direct constructor assertions retained.

The Futures non-array diagnostic calls the canonical native JSON stringifier.
It escapes lone surrogates, maps nonfinite numbers according to JSON.stringify
and reports errors through the existing catch boundary. There is no JSON
projection before native shape checks, alias reads or stringification.

`tests/support/private_rest_native.rs`, included by the native `private_classes`
target, constructs actual native dependencies, venue owners and signed HTTP
clients. Only terminal HTTP and scheduling are scripted. The maintained source
observer evaluates the actual TS methods, enums, get-key, safe math and base log.
The diagnostic, unread PM-child and unread array-child regressions each compile
and fail on master at the removed projection panic. A separate malformed-PM
case compares nested nonfinite metadata with the same source guard. The client-ID
lookup witness starts with an empty Store, performs signed REST reconciliation
and checks raw info plus native quantity/timestamp bits.

Run the native binding cohort from `sdk-rs`:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-binance --features native \
  --test private_classes private_rest_native -- --nocapture
```

Use the canonical accepted-set `--measure` with the six surviving `private_rest`
function owner selectors and the `response_shape` / `response_message` module
selectors. Full discovery, compiler provenance and same-file local-helper checks
remain enabled. `digits` and `values` retain their byte-identical bodies and
individual role reviews; their coarse source hashes are refreshed after reviewing
the surrounding changes. Measurement exit zero alone is not a passing receipt.

This is a finite cohort, not whole-venue equivalence. Callable aliases, other
modules, executable-object profiles and the existing typed UTF8 model boundaries
retain their separate evidence and ownership.

Error-message selection reads the canonical retained `Thrown` message instead of
the UTF8 diagnostic field. `ExchangeState::emit` sends the original native value
to `ExchangeErrorInput::Raw`, the same formatter used by the JS base facade.
The constructed native emitter and first PM snapshot-failure regressions compare
D800/DC00 units, exact effect counts, signed HTTP ordering and cached fallback.
The PM expectation uses the maintained source guard; the regular-order failure
prevents the later conditional HTTP call. Typed console/log adapters retain their
separate UTF8 boundary; this selected fix does not widen that shared API.

The #974 PM warning-template change replaces UTF8 `format!`/`text()` with
UTF16 prefix concatenation and `to_js_string()`. Review against #970 confirms
that nested `digits` and `values` bodies are byte-identical and keep their
individual roles. Both whole-file pins now cover that retained warning path.
