# Crypto.com envelope and body helpers

**2 rows pending → ported; 0 pending → divergent**:
`isAuthenticatedEnvelope` and `stringifyResponseBody`.
Independent TOML censuses change Crypto.com from 30 ported / 18 divergent /
89 pending to **32 ported / 18 divergent / 87 pending**. See `census.json` and
`merge-census.json`. The census floor is six manifests / 137 rows, ending at
`cryptocom.ws-public.ts.toml` / `CryptocomPublicWebSocket.dispose`.

## Commits and measured inputs

- Fresh branch `feat/cryptocom-api-envelope-body` starts from reviewed integration
  `6e5c3103e731ff6cb5c7198861d53ff0b70514c2`.
- Implementation and control base:
  **`fabc4897d044028134baeb340e47a802f2905595`**.
- Final merge **`8cac2ac8ce2e76940242b9c72cf838789534c083`** merges integration
  `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a`.
- `merge-comparison.json` records scoped source/configuration diff exit 0 for
  this crate's source, tests and Cargo file, actual source API and the named
  shared dependency source/Cargo/configuration paths. It separately records
  workspace Cargo.lock diff exit 1: an ApexOmni base64 dependency entry was
  added. Clean native/compile/lint/format/extractor gates were rerun at the merge
  SHA; the two mutation measurements remain bound to fabc4897d0.
- Comparisons cover the explicitly named paths, not every environmental input.
  No old gate is relabeled as a new run. No separate unreviewed feature branch
  is an ancestor. The subsequent evidence-only commit changes no production
  source, tests, manifests or build configuration.

## Parity and explicit models

Envelope validation first applies the ported record guard, then reads
`method`, `api_key`, `params`, `sig`, `nonce` in source short-circuit order.
Method/key comparisons use complete UTF-16 primitive strings with no coercion.
Params use the record guard; sig accepts every primitive string, including
empty strings and unpaired units; nonce accepts every native number, including
zero, NaN and Infinity, because the source checks its type rather than finiteness
or truthiness. Getters and Array.isArray failures retain their original native
error. The `EnvelopeValue` model preserves exactly the source comparison
classes: opaque object handles, lossless strings, full numbers and the uniformly
rejected remaining class. Arrays/proxies remain objects for the array boundary.

Response stringification returns primitive UTF-16 strings unchanged without
calling either builtin. Other native values are forwarded unchanged to
JSON.stringify, including values that return undefined. Only a thrown stringify
failure invokes String with that same original value. The JSON error is caught;
the String result or its original error is retained. Native Option output
preserves the real runtime undefined return even though the source annotation
declares string. `ResponseBody` partitions primitive strings from the complete
remaining native carrier domain.

Both real public functions and both public value models are explicitly mapped
as explained function/model splits in the API manifest. No target is fabricated
or exempted. These are complete function-body claims at native classification,
Array, property, JSON and String dependency boundaries; builtin implementations,
API factory/interceptors and caller wiring remain pending.

## Native witness and controls

`api_envelope_body_matches_actual_typescript` executes the complete unchanged
actual source arrows, including the source record predicate. Array.isArray,
JSON.stringify, String and property getters execute their actual source/native
operations. Their observed results are supplied to Rust callbacks as dependency
values; those observations are not proof of newly ported builtin implementations.

Rust checks decoded native booleans, UTF-16 string/undefined outcomes, original
error identity (`Rc::ptr_eq`), argument/receiver identity, and ordered effects.
All native assertions precede independent inventory/ingress checks. No Rust
result is re-encoded for comparison. The prescribed expectation is **33 envelope
/ 22 body = 55 native cases**, ending at body case21 `mutating-toJSON`. This
is a case floor, not exhaustive runtime enumeration.

The floor includes nullish/primitive/function/array roots, revoked proxies,
strict method/key matching, surrogate keys/signatures, boxed/rejected field
types, NaN/Infinity/negative-zero nonces, each throwing field getter, a mutating
getter, primitive string bypass, stringify undefined, ordinary JSON output,
bigint/circular/getter/toJSON failure, fallback failure and toJSON mutation
visible to the later String call.

Both controls below were measured at **fabc4897d0** plus only the individually
recorded patch. Each mutant compiled and failed at its named native runtime
assertion, then passed all 55 cases after exact source restoration.

| Control | Named assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| Invert nonce number-type predicate | `CRYPTOCOM-ENVELOPE-BODY-NATIVE envelope case6 valid` | 101 | 0 |
| Return undefined for primitive string | `CRYPTOCOM-ENVELOPE-BODY-NATIVE body case0 empty` | 101 | 0 |

There are no survivors among these two controls. The fixed control floor stops
at the second control, primitive-string output; no exhaustive mutation coverage
is claimed. Exact patches, commands, variants, SHAs and raw logs are retained
in the mutation JSON files and `gates.json`.

## Sub-agent review

`review_auth_witness` found no blocker in exact fabc4897d0 at the declared host
boundaries: correct getter order, strict comparisons, native classification,
undefined preservation, fallback order, same-body forwarding and original
error identity. It verified that all four real public function/model targets
are mapped. Its independent scoped diff confirmed reviewed source, tests,
oracle, API manifest and actual TypeScript source remain unchanged at 8cac2ac8ce.
It inspected both exact patches and raw failure/restoration logs, confirming
compiling named runtime failures 101, restored 0 passes reaching the 55-case
floor, and no survivors among these controls. It ran no Cargo and did not
claim unfinished merge gates or relabel premerge measurements.

## Not run

Full Crypto.com suites, other crate suites, workspace compilation/suites,
canonical parity index validation, wasm runtime tests, live exchange tests,
prior slices' controls and CI were not run. Host and wasm clippy are separate
scoped compilation/lint measurements, not wasm execution proof. Mutation
controls were not rerun after the byte-unchanged source merge; their measured
exits remain bound to fabc4897d0. Clean merge gates are recorded separately.

## Measured gate ledgers

Exact commands, elapsed times, SHA and dirty variant are in the JSON ledgers.
Cargo runs were sequential, scoped to `safe-cex-venue-cryptocom`, with `--jobs 2`.
Native witnesses used the supplied TypeScript module path.

### Implementation — `fabc4897d044028134baeb340e47a802f2905595`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| envelope-mutation | 101 | [envelope-mutation.log](envelope-mutation.log) |
| envelope-restored | 0 | [envelope-restored.log](envelope-restored.log) |
| body-mutation | 101 | [body-mutation.log](body-mutation.log) |
| body-restored | 0 | [body-restored.log](body-restored.log) |
| check | 0 | [check.log](check.log) |
| host-clippy | 0 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 0 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| writer | 0 | [writer.log](writer.log) |
| extractor | 0 | [extractor.log](extractor.log) |

### Final merge — `8cac2ac8ce2e76940242b9c72cf838789534c083`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native | 0 | [merge-native.log](merge-native.log) |
| check | 0 | [merge-check.log](merge-check.log) |
| host-clippy | 0 | [merge-host-clippy.log](merge-host-clippy.log) |
| wasm-clippy | 0 | [merge-wasm-clippy.log](merge-wasm-clippy.log) |
| fmt | 0 | [merge-fmt.log](merge-fmt.log) |
| writer | 0 | [merge-writer.log](merge-writer.log) |
| extractor | 0 | [merge-extractor.log](merge-extractor.log) |

Both writer outputs report Updated 0 manifests; both extractor checks report
Would update 0 manifests. Output texts were verified in addition to the measured
process exits.
