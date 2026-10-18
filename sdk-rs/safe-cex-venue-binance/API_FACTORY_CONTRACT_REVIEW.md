# API factory contract cohort

The five previously missing owners retain exact compiler APIs. Existing
`ApiFuture`, `RequestInterceptor`, `ResponseInterceptor` and `RemoteSignCall`
alias records remain separate and unchanged.

| Owner | Review and execution |
| --- | --- |
| `QuerySerializer::serialize` | Four executed native output profiles: empty object, repeated array fields, UTF8 percent encoding/null and nested keys/Boolean. The returned string is read directly. |
| `RetryRead::matches_read` | Eight executed native Boolean profiles cover both policies, GET/HEAD case normalization, POST/DELETE at the literal path, query stripping and distinct/missing paths. |
| `optional` | Eight executed native property outputs distinguish nullish receiver, own undefined, missing, own null, Boolean, literal UTF16 text and retained child identity/later mutation. |
| `endpoint` | Specific non-decoder role: selects a generated static endpoint using internal literal keys. |
| `utf8_hmac_sha256_hex` | Specific non-decoder role: outbound UTF8 authentication digest over an already assembled payload; it does not decode a response. |

The two role classifications do not execute their algorithms. Separate real
production bindings establish the bounded effects: the new native factory test
constructs `native_dependencies` and `ExchangeOwner`, injects one terminal network
reset, then observes two actual unsigned listen-key POSTs and the returned key.
It next executes the live income reader and checks the actual signed HTTP query's
symbol percent encoding and limit. There is no factory, client, signer or retry
replacement. Existing real Spot constructor/private onOpen observes the parent
key/secret/clock and actual transmitted HMAC. The real native Futures leverage
error witness checks the failure interceptor's eventual raw error report.

The maintained reference is `binance.api.ts`/`binancepm.api.ts`/`binancespot.api.ts`,
`retryReadsOnly.ts` and the Spot private authentication body. This finite decoded
profile does not claim Node Buffer/key-object HMAC coercion, executable getters,
all query shapes or whole-venue equivalence. Classification does not grant a
nominal raw API a typed/non-decoder exemption: both raw nominal methods execute
observed contracts. No alias, receiver, local-function or cancellation guard is
changed; production sources remain exact.

Run from `sdk-rs`, serially:

```sh
node safe-cex-venue-binance/scripts/api-factory-contracts.mjs
python3 safe-cex-venue-binance/scripts/api-factory-controls.py
cargo clippy --locked --jobs 2 -p safe-cex-venue-binance --features native --lib --test private_classes --test json_error_binding -- -D warnings
cargo test --locked --jobs 2 -p safe-cex-exchange --all-features --test cancellable_future_inventory
```

The canonical runner preserves complete compiler/dependency discovery and checks
all nine scoped receipts. Its pre-contract `--expect-unclassified` mode requires
five compiled missing-contract failures by exact name and four passing aliases.
The independent control runner disconnects each of the five production helpers,
requires its named actual native binding to fail after compilation, restores exact
source bytes, and requires that test to pass. Compiler errors and empty test
selections are refused. The canonical GREEN comprises three executed native
outputs, two individual role classifications and the four existing aliases.
