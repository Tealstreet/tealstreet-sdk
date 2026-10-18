# Crypto.com control-flow contract batch

This reviews fifteen original native/default census owners individually. Their
complete Rust bodies and corresponding TypeScript caller roles were read. Each
constructs outbound values, selects/awaits native continuations, accesses supplied
properties or reports errors. The fresh proposal report at master
31917cde56c6aec10a1605ceee1af5c6906e0be6 supplied exact compiler APIs with no blocked
targets; it did not authorize classifications or supply behavioral expectations.

The per-owner reasons and native observations are recorded in the Crypto.com
contract shard. Existing source boundaries remain those in
[MANIFEST-SPEC](../parity/MANIFEST-SPEC.md#cryptocom-helper-value-boundaries).

| Owner within safe_cex_venue_cryptocom | Reviewed role | Native witness |
| --- | --- | --- |
| order_writes::log | Diagnostic rendering/dispatch | order_writes: native log message/severity and reporting failures |
| order_writes::rethrow | Error propagation | order_writes: original error, mandatory logging and replacement error |
| order_writes::caught | Synchronous-error continuation selection | order_writes: eager logging and original future/error |
| order_writes::finish | Await/opaque property access/outbound result | order_writes: original order ID, actual array cardinality and response/error handling |
| order_writes::construct | Outbound Error construction | order_writes: constructor-before-operand order, message and error identity |
| order_writes::update_value | Presence-based property selection | order_writes: original amount/price carriers, repeated reads and failures |
| order_writes::listen | Consumer delegation | order_writes: socket receiver, argument/callback identity and original result |
| rest_reads::begin | Request dispatch | rest_reads: original endpoint/params, receiver, eager call and errors |
| rest_reads::catch | Catch/report continuation | rest_reads: diagnostics, original errors and swallow/rethrow behavior |
| rest_reads::caught_begin | Synchronous-error continuation selection | rest_reads: pre-poll catch effects and selected completion |
| rest_reads::parsed | Ordered supplied-parser dispatch | Direct parsed_native_contract: seven parser-before-field, number-bit and abrupt-order cases |
| rest_reads::nullish_field | Nullish property selection | rest_reads: original fields/fallbacks, falsey values and read order |
| rest_reads::zero_position | Outbound position assembly | rest_reads: all native fields/default bits, original IDs/enums and construction order |
| class_mappings::imported | Imported property access | Direct imported_native_contract: receiver/child/error identity and order |
| class_admin::optional | Optional property-access adapter | Direct optional_native_contract: nullish/falsey distinction, child/error identity |

Twelve order_writes/rest_reads helpers execute through the
actual unchanged public native caller bodies in the existing source-comparison
targets. Those targets independently admit 91 and 68 scenarios respectively,
assert native fields, numeric bits, original references/errors and effects, and
execute complete unchanged actual TypeScript methods. Each selected private body
also has an individual corruption control. Composition is explicit here: these
twelve witnesses are not new direct private-function tests.

The three direct witnesses call their actual private bodies with concrete shared
Js/Thrown carriers or literal UTF16 units. Imported access checks global-before-
field resolution, original enum receiver/child/error identity, missing properties
and abrupt order. Optional access distinguishes nullish from falsey non-nullish
carriers and preserves original getter failures.
The parser-dispatch witness resolves a supplied native function before the
field accessor, checks its original input and undefined receiver, observes -0,
Infinity and NaN results, and preserves original failures at global lookup,
field Get, invocation and numeric-result extraction. A separate seven-case
executed-count assertion protects its full walk. The shared SocketOps fixture is
extracted unchanged into a test-only helper;
unused supplied capabilities panic. Nine existing socket helper tests verify
that extraction without changing production operations. Review additionally
identified silent truncation holes in the optional nullish/falsey and imported
abrupt-stage walks. Fixed counts of two, three and two now protect their original
inputs and expectations, with three named truncated-walk failures and restored
passes.

These roles grant no downstream admission permission. In particular, parsed
dispatches a supplied parser; its grammar/coercion remains separate. finish
forwards an opaque order ID into an outbound array, and zero_position constructs
an outbound record; neither validates an incoming wire schema. The switch
conversion class_mappings::member remains a separate review. Generic host/Clone behavior, aliases and construction records
are not exempted.

Run sequentially from sdk-rs, with the actual TypeScript compiler environment:

```sh
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib accepted_control_flow
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --lib accepted_helpers
cargo test --locked --jobs 1 -p safe-cex-venue-cryptocom --test order_writes --test rest_reads
python3 safe-cex-venue-cryptocom/scripts/control-flow-controls.py
node safe-cex-venue-cryptocom/scripts/control-flow-contracts.mjs
```

The control runner requires fifteen distinct body corruptions and four truncated
fixture walks (parser dispatch, optional nullish/falsey inputs and imported
abrupt stages) to compile, fail their named
native assertion with exit
101, restore exact bytes and pass again. The result-array mutation fully reads
the original ID before constructing an empty array. Compiler errors do not count,
and expected native values are not revised from mutation output.

Selected validation passed: all four new direct witnesses, nine unchanged socket
helper witnesses after the test-only fixture extraction, the three actual-source
composition targets (252 admitted scenarios), all twenty body controls and the
five fixture-walk controls. The fresh closed selector produced twenty explicit
classifications and zero failures. Integrated readiness is recorded in the
[PR validation record](https://github.com/Tealstreet/tealstreet-sdk/pull/394).

The closed fresh selector accounts for precisely these twenty owners as explicit
non-decoder classifications, with zero runtime decoder probes. Compiler receipts
remain separate from actual native evidence and compiling body controls. Other
original/new owners, aliases, construction, locals, features and WASM contract
execution remain separate obligations. No full-package completion is claimed.

The test-only `account_admin.rs` and `account_envelopes.rs` modules had no
production caller and were removed with their owners (wave2-vacuous-tests). The
validation paragraphs above record the larger batch as it passed before that
removal; the selector, control runner and contract shard now hold the fifteen
owners in the table.
