# Crypto.com signing helper roles

This slice individually reviews five existing native/default functions. They
serialize existing signing data, compute outbound cryptographic outputs, or
construct outbound request fields. They do not parse an inbound DTO or supply a
validation token. SigningValue and downstream decoders retain independent
review obligations. Existing signing-data and UTF8 boundaries in MANIFEST-SPEC
remain unchanged; these classifications do not claim arbitrary executable JS
input, lone-surrogate strings, HTTP serialization or dispatch parity.

| Owner | Reviewed role | Native witness |
| --- | --- | --- |
| params_to_string | Preimage serialization of existing JSON data | tests/signing.rs directly calls it for 280 independently admitted live-source comparisons, observing native strings or coercion errors |
| generate_signature | Outbound signature construction | tests/auth.rs directly calls it and compares native HMAC output/error against the actual source |
| signature_from_params | Preimage formatting and HMAC over already serialized params | Executed by the actual public signature/auth builders, with its own body-specific HMAC corruption control |
| generate_authenticated_request | Outbound request construction retaining captured params | tests/auth.rs directly observes every native field, params reference identity, clock count and coercion error |
| generate_ws_auth_message | Outbound WebSocket auth-message construction | tests/auth.rs directly observes every native field and the two ordered clock reads |

The live signing/auth tests retain their independent input inventories, case
counts and executed comparison counts. They execute the unchanged complete TS
signing module; no expectation is harvested from native output. The auth corpus
has 42 cases across the three public builders. The private HMAC formatter has
its own compiling body corruption, so its evidence is actual composition plus
that discriminating control rather than privacy or parent naming.

Run sequentially from sdk-rs, with SAFE_CEX_TYPESCRIPT_PATH set to the live
TypeScript compiler:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test signing --test auth
python3 safe-cex-venue-cryptocom/scripts/signing-role-controls.py
node safe-cex-venue-cryptocom/scripts/signing-role-contracts.mjs
```

The controls individually corrupt the serializer cutoff, the public signature
serializer call, the private HMAC result, lazy request nonce evaluation and the
second WS clock read. Each must compile, fail a named native value or clock
assertion with exit 101, restore exact source bytes, and pass again. The closed
selector independently checks fresh compiler API identities and emits five
non-decoder classification receipts. Those receipts are not runtime tests.

Selected validation passed: the two live native targets (280 serializer
comparisons and 42 auth cases), all five compiling corruption/restoration
controls, and five fresh API classification receipts with zero failed owners.
Integrated readiness requires the full scoped suite, strict host/WASM
all-target Clippy, formatting, the manifest invention/status gate and a final
fresh five-owner check. The [PR validation record](https://github.com/Tealstreet/tealstreet-sdk/pull/348)
records those results against merged master. No production code or manifest
status changes are proposed. Scope is these five original native/default
owners; other owners, aliases, construction, locals, features and WASM contract
execution remain separate obligations.
