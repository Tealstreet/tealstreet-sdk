# WOO public orderbook, candle and interval helpers

This slice ports eight additional pending rows, stopping at
canonicalizeWooLegacyCandles in the manifest mapping. The test source-case
iteration stops at canonicalizeWooV3TickerStats: five new native groups and 28
source cases, plus the previous five groups and five source cases.

Independent native expectations cover every level/candle projected field,
ordered keys, exact canonical strings, mapper identities, unchanged inputs,
empty-symbol errors, twelve fixed durations, and five blocked interval inputs.
A native operation trace requires Object.freeze lookup before duration access
and then actual freeze invocation. The source witness compiles maintained
TypeScript and invokes actual functions. Fixtures remain a finite ordinary
record/ASCII floor, with required opaque host operations for exotic behavior.

Parent: `7dbea9b2e62a8f02300ac16b46d2f7a72a4b7731` (unmerged PR 7858).
This is a fresh stacked branch; that unreviewed feature is an ancestor.
Integration base: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
The original receipt applies only to its stated SHA; it does not cover this slice.

Sub-agent static review found no production-body mismatch in the eight added
roots. Its floor is these eight roots and five new assertion groups. Metadata,
compilation, runtime gates and mutation controls remain unmeasured at this step.

Canonical declarations are linked in MANIFEST-SPEC:
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Gate receipt

Measured code SHA: `e0d93f262c5a3793c5b47a940ae6be2b028a81ea`.
`tomllib` over 21 WOO manifests, stopping at woo.ws-public, gives
202 ported / 308 divergent / 22 pending. Exactly eight new rows moved
pending→ported relative to the stated parent; 16 relative to integration.

Measured exits at this code SHA:

- Scoped WOO `cargo check --all-targets --jobs 2`: 0.
- Focused `public_canonical` Cargo target, jobs 2 and one test thread: 0,
  ten passed (five existing and five new groups).
- Extractor check: 0, output 7341 symbols / 512 files, would update 0 /
  add 0 / orphan 0.
- WOO-only CI wiring target: 0, two passed.

The five-family compiling mutation batch exited 101: five new named native
assertions failed (candle sort, 1m duration, bid/ask canonical placement,
level amount, V3 mapper). The five existing groups passed. Exact restoration
then exited 0, ten passed. A separate freeze-omission mutation initially had
an invalid Cargo CLI separator, exit 1; that invocation proves nothing. The
corrected focused invocation compiled and exited 101 at
`VALUES-NATIVE freeze callee before duration lookup`; exact restoration exited
0, one passed / nine filtered. No survivor in this bounded control set.

Sub-agent review independently confirmed the eight row moves, source order,
operation trace, raw gate/control output and exact restored source/test bytes.
It ran no Cargo gates. The receipt commit changes only this document; all gate
inputs remain byte-identical to the measured SHA. No full venue/workspace
suite, clippy, wasm, network or live venue check was run.

The combined batch does not establish independent sensitivity for every edit:
the level-field mutation could mask the side swap. An additional isolated
bid/ask swap was therefore measured at receipt SHA
`213927a9209d8092079faaf2024a0c8cd2fc0c09` (gate inputs byte-identical to
e0d93f262c5a3793c5b47a940ae6be2b028a81ea). It compiled, then exited 101 at
`VALUES-NATIVE order book`; exact restoration exited 0, one passed / nine
filtered. This is a separate measured control, not inferred from the batch.
