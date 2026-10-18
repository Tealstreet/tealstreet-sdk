# Request-weight output contracts

The three previously unclassified functions now execute finite production
output contracts. They supplement the seven existing callable/error aliases;
no alias, receiver, local-function or cancellation permission changes.

| Owner | Native observations |
| --- | --- |
| `get_response_header` | Absent headers skip all reads; callable `get` is read twice; callable-to-noncallable transition raises TypeError; exact/lower/upper property precedence preserves falsy values and terminal null; callback errors preserve their Rc identity. |
| `observe_request_weight` | Numeric 429/418 select retry-after; string status does not; invalid/nonpositive durations use 60 seconds; used-weight threshold is inclusive at 1920; header and pause failures preserve identity and stop effects. Durations are read as binary64 bits. |
| `type_error` | Actual native error name, message UTF16 units and absent code/response/original_error for five literal message profiles. |

The maintained reference is `binance.exchange.ts`'s `getResponseHeader`,
`observeRequestWeight` and constructor installation. Independently authored
expected events and fields are checked before diagnostic serialization.
`Option<Value>` is this extraction's existing boundary: terminal explicit null
and absent values remain distinguishable in the direct header profile. These
finite cases do not claim arbitrary executable headers or all JS values.

`request_weight_binding::native_response_weight_pauses_actual_owner_at_threshold`
constructs `native_dependencies` and the actual `ExchangeOwner`, executes signed
order HTTP requests, then reads the live owner's pause timestamp. Four header
profiles cover below/at/above threshold and invalid text. Futures pauses only at
or above threshold. PM and Spot do not install these guards in their maintained
constructors and remain no-pause controls. Only terminal HTTP and runtime ports
are scripted; there is no replacement client, signer, interceptor or owner.

From `sdk-rs`, run serially:

```sh
node safe-cex-venue-binance/scripts/request-weight-contracts.mjs
python3 safe-cex-venue-binance/scripts/request-weight-controls.py
cargo test --locked --jobs 2 -p safe-cex-venue-binance --features native --test private_classes request_weight_binding::native_response_weight_pauses_actual_owner_at_threshold -- --exact
cargo clippy --locked --jobs 2 -p safe-cex-venue-binance --features native --lib --test private_classes -- -D warnings
cargo test --locked --jobs 2 -p safe-cex-exchange --all-features --test cancellable_future_inventory
```

The controls independently break header dispatch and threshold selection and
require the named real HTTP-binding failure after compilation. The scalar error
constructor is additionally faulted through its actual private production
function in the canonical driver. Every control restores exact source bytes and
requires a passing rerun; compiler failures never qualify. The canonical runner
retains full compiler/dependency discovery. `--expect-unclassified` is for the
pre-contract baseline, requiring all three missing owners by name and all seven
existing aliases to pass. Diagnostic `--measure` exit zero alone is insufficient.
