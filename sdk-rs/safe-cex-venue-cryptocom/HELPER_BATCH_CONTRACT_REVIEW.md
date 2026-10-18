# Crypto.com helper contract batch

This reviews seventeen original native/default census owners individually. Three
order mappings return concrete venue enums and require actual decoder probes.
Fourteen other functions compare already captured primitives, assemble outbound
text or dispatch supplied host operations. Each
has its own body-specific role review and direct native witness; none receives a
privacy, generic-host, parent-receiver or downstream-decoder exemption.

Compiler proposal discovery is separate from executed behavioral evidence. The
fresh proposal report at master 77fa5af694be326259fe2a603c61d8043cf23918 had no
blocked targets. Source functions and caller bodies were read directly, including
cryptocom.exchange.ts order switches, cryptocom.api.ts error formatting, and the
public/private websocket bodies. The manifest statuses and existing boundaries
remain unchanged. Cite the canonical boundary definitions in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary) and
[the native host boundary](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary).

| Owner within safe_cex_venue_cryptocom | Reviewed role | Direct witness |
| --- | --- | --- |
| order_mappings::map_order_type | Enum-returning switch | native_map_order_type: 18 independently authored Option/UTF16 cases and native variants |
| order_mappings::map_time_in_force | Enum-returning switch | native_map_time_in_force: 16 cases and native variants |
| order_mappings::map_stop_trigger_type | Enum-returning switch | native_map_stop_trigger_type: 16 cases and native variants |
| order_mappings::equals | Strict primitive comparison | accepted_equals::native_strict_comparison: 24 cases |
| private_order_mappings::equals | Strict primitive comparison | Its own accepted_equals::native_strict_comparison: 24 cases |
| socket_handlers::get | Supplied property Get | Original native child/error identities and getter effects |
| socket_handlers::method | Method binding/dispatch | Callee capture, original receiver/arguments/result, evaluation and abrupt order |
| socket_handlers::global | Global binding/dispatch | Captured callee, undefined receiver, argument/result/error identities and order |
| socket_handlers::own | Borrowed receiver binding | Actual Js Clone retains receiver identity; original result and failures |
| socket_handlers::math | Math method binding | Original Math receiver; global/method/arguments/invoke order and failures |
| socket_handlers::parsed_field | Supplied parseFloat dispatch | Global resolution before field Get; original input/result/errors |
| socket_handlers::parsed_or | Lazy fallback selection | Eleven independently chosen truthy/falsey results, original carriers, default number bits and error |
| socket_handlers::concat | Outbound text rendering | UTF16/lone-surrogate units, empty text, number rendering and original coercion error |
| socket_handlers::caught | Catch/report control flow | Success no-op; original error and parent receiver, message units, reporting order and replacement errors |
| api_error::append | Outbound message assembly | Existing prefix, Unicode pairs, empty append and separator order |
| api_error::optional_get | Optional property access | Nullish short circuit; child/getter-error identity and falsey non-nullish access |
| api_error::http_message | Outbound HTTP diagnostic | Original status/body carriers, coercion order, separator/UTF16 output and both failures |

The enum expectations were authored from the source switches before execution.
Each observer matches actual native enum variants, independently of Serde names.
The UTF16 comparisons include NUL, Unicode pairs, lone surrogates and distinct
normalization sequences. Literal/key/prefix inputs retain the actual Rust str
domain; None in the order mappings is the existing comparison-equivalent
non-string fallback, without coercion. No raw JSON decoder or dynamic JS object
admission is implied by the fixture transport.

The socket witnesses use shared Js/Thrown for concrete carrier/error referents
and explicit SocketOps callbacks. They call each actual private helper directly.
They do not replace a parseFloat implementation contract with a dispatcher test
or prove arbitrary host/Clone behavior. Error formatting similarly uses an
explicit ApiErrorHost with native carriers and independently specified template
results. 
The maintained control runner has seventeen distinct body mutations and three
truncated fixture-walk mutations. Each must compile, fail the named runtime
assertion with exit 101, restore exact source bytes and pass again. A compile
failure is not proof. No expected native value is revised from mutant output.
The count controls cover all three loop patterns: enum mappings, private
comparison and parsed fallback cases.

Run sequentially from sdk-rs:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib accepted_helpers
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib accepted_equals
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test accepted_mapping_variants --test order_mappings --test socket_handlers --test api_error
python3 safe-cex-venue-cryptocom/scripts/helper-batch-controls.py
node safe-cex-venue-cryptocom/scripts/helper-batch-contracts.mjs
```

Selected validation passed: all twenty new direct native tests, the unchanged live
source mapping/socket/error/lifecycle targets, all twenty body controls and six
fixture-walk controls, and the fresh closed selector. It produced three actual native
runtime decoder probes, seventeen separate compiler classification receipts and
zero failed owners. Integrated readiness results are recorded in the
[PR validation record](https://github.com/Tealstreet/tealstreet-sdk/pull/379). Direct native evidence and body controls remain required
for every one of the twenty owners; classification receipts alone are not runtime
evidence. Other original/new owners, aliases, construction, local scopes, feature
configurations and WASM contract execution remain separate obligations. No
full-package completion is claimed.

The test-only `public_lifecycle.rs` and `private_lifecycle.rs` modules had no
production caller and were removed with their owners (wave2-vacuous-tests). The
validation paragraphs above record the larger batch as it passed before that
removal; the selector, control runner and contract shard now hold the seventeen
owners in the table.
