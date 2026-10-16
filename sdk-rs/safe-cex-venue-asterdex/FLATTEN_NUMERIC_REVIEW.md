# Flattened numeric decoder audit

This review covers the twelve numeric/flatten declarations in the AsterDEX
census: the public market-filter, market, position, order, ticker and websocket
records, and each record's ordinary derived lexical `Wire`. The governing
contract is [numeric ingress](../parity/MANIFEST-SPEC.md#numeric-ingress-defect).

`tests/fixtures/private-wire-numeric.json` contains six independently authored
valid objects. Each varies a native numeric field through `1`, `1.25` and `-0`;
fixed expectations are the corresponding binary64 bits, a distinguishing
native string field and a retained unknown string extension. The fixtures use
the maintained TypeScript field names and required fields.

`scripts/private-wire-numeric-source.cjs` checks these eighteen objects against
the maintained TypeScript record schemas and executes `JSON.parse` to observe
the number bits. TypeScript interfaces are erased: this is a source-shape and
JavaScript number witness, not a claim that TypeScript has a runtime decoder.

`tests/private_wire_numeric.rs` exercises the six actual public Rust decoders
through JSON text, JSON bytes, owned Value and qualified borrowed Deserialize.
It checks native number bits, the distinguishing string and the unknown field,
and rejects nonobject outer shapes. The existing `live_types` numeric tests
provide broader per-field and unknown-number coverage.

`scripts/private-wire-numeric-controls.py` copies production source into an
isolated workspace under `sdk-rs/target`. After each exact lexical declaration,
it injects a diagnostic that invokes that declaration's own `Wire` decoder on
raw JSON text. It retains the actual derived decoder, field attributes,
flattening and field types. It measures each decoder independently, checks
native fields and number bits, and runs the public tests after each exact-byte
restoration. Compiler errors do not count as measured decoder failures.

The copied lexical diagnostics are measurement only. They are not executable
accepted-contract inputs, classifications, or permission to exempt local
owners. A passing public record does not discharge its inner decoder's
independent inventory and contract obligations.

The final compiled control changes only the public websocket mapping from
`capital_e: w.capital_e` to `capital_e: 0.0` in the isolated copy. The public
acceptance assertion must fail with the existing numeric-ingress bit-loss
error for `$.E`, and exact-byte restoration must pass. The private
diagnostic still observes the original `Wire` field, so this measures an output
bit-loss regression rather than an unused-field compilation failure. The live
production source must remain byte-identical throughout.

Run serially with the repository's locked Rust toolchain and the maintained
TypeScript dependency path:

```sh
python3 sdk-rs/safe-cex-venue-asterdex/scripts/private-wire-numeric-controls.py
```

The runner uses one Cargo job and the existing accepted-set build cache. Do not
run another Cargo process against that cache concurrently. Compiler output and
measurement logs remain outside committed source.

On master `5508a17eca`, the locked Rust 1.99 run passed all six private decoder
measurements and every six-test public baseline/restoration. The public tests
observe 72 successful decodes per full run. The compiled bit-loss control failed
only `websocket_raw_numeric` (five passed, one failed), with the exact existing
numeric-ingress error for `$.E`; restoration passed all six tests. Live
`src/types.rs` remained byte-identical. The source-shape/JSON.parse check passed
all eighteen cases under Node 26.7.0 on arm64. No numeric failure was measured,
so this audit makes no production decoder repair.
