# Crypto.com API guard slice

**5 rows pending → ported; 0 pending → divergent**:
`isRecord`, `parseJsonObject`, `isCryptocomAuthenticationErrorCode`,
`isCryptocomAuthenticationFailureMessage`, and
`shouldSuppressValidationAuthErrorEmission`.
The independent TOML census changes Crypto.com from 25 ported / 18 divergent /
94 pending to **30 ported / 18 divergent / 89 pending**. See `census.json` and
`merge-census.json`. The census floor is six manifests / 137 rows, ending at
`cryptocom.ws-public.ts.toml` / `CryptocomPublicWebSocket.dispose`.

## Commit and parent boundaries

- Fresh branch `feat/cryptocom-api-guards` starts from reviewed integration
  `50b1dac9770bb23a8e69e1101945c337c468c194`.
- Implementation and mutation-control base:
  **`259a993ab27fd0aa5b50fb7ab5824258bc37bbe9`**.
- Final merge **`ec163d264d0a22e1e71002cd350497c24870b2b9`** merges integration
  `cb78ccd661ad20629105c9588cc3970f9778b6a5`.
- `merge-comparison.json` records exact `git diff --exit-code` exit 0 between
  these commits for this crate's source/tests/Cargo file, actual source API,
  and the named shared dependency source/Cargo/configuration paths. This is
  byte identity over those paths, not every environmental input. Old measured
  exits retain their original SHAs; final-merge reruns have their own ledger.
- No separate unreviewed feature branch is an ancestor. The subsequent
  evidence-only commit changes no production source, tests, manifests or build
  configuration.

## Parity boundaries

The source unknown-value guards use complete comparison-equivalent native
partitions: `is_record` accepts Some only for `typeof value === 'object' &&
value !== null`, while the rejected class is None. Arrays, boxed values, maps,
dates and proxies remain objects; the Array.isArray dependency determines the
array result and can throw its original native error. It is invoked once only
after the guard, and its result is negated. No JSON-only production model is
used to narrow the source unknown domain.

The JSON-object helper forwards the complete UTF-16 input to its parser, tests
the unchanged native parsed value once, and returns that same value by identity
only for a record. Parser and predicate failures both return native undefined,
matching the source catch. The parser implementation is a dependency boundary;
this row does not claim a new JSON parser implementation.

Authentication-code checking invokes String conversion exactly once with the
unchanged native input, preserves its original error, and strictly compares
all returned UTF-16 units to 40101. Conversion behavior is provided by the
source String dependency, not claimed as a newly ported builtin. The message
helper independently implements the source primitive-string guard and exact
case-sensitive contiguous UTF-16 substring match, including unpaired surrogates
and the source's longer-code-prefix behavior.

Suppression calls the ported code checker first. A false result or conversion
failure skips the receiver property entirely. The property is read once only
after a true result; strict equality accepts only native boolean true. The
Some(true)/Some(false)/None representation preserves the full comparison
partition, including all truthy non-booleans that the source rejects.

These are five function-body claims at the declared native classification,
builtin and property boundaries. The API factory, interceptors, callers and
host dependency implementations remain pending. There are no new public
helper enums or exemptions. Every renamed target is a real public function
explicitly mapped in the source manifest.

## Native witness and runtime controls

`api_guards_match_actual_typescript` executes the unchanged complete actual
source arrow initializers and reads the actual authentication-code constant.
The source builtin wrappers call real Array.isArray, JSON.parse and String.
They capture dependency results/failures and argument identity. Those native
dependency observations are supplied to Rust callbacks as dependency values;
they are not advertised as proof of Rust builtin implementations.

Rust asserts decoded native booleans, parsed-value and original-error identity
using `Rc::ptr_eq`, and ordered effects. The source checks returned/thrown
identity before transporting observations. No Rust result is serialized for
comparison. Every native result/effect assertion precedes the independent
inventory and ingress checks. Fixed expectations are **20 record / 19 parse /
19 code / 16 message / 13 suppression = 87 cases**. This case floor ends at
suppression case12 `flag-throw`; it is not exhaustive runtime enumeration.

The floor covers nullish and primitive inputs, boxed/native objects, arrays,
null-prototype objects, ordinary/revoked object/array/function proxies,
unpaired surrogates, invalid JSON, duplicate and prototype-named JSON keys,
parser/predicate failures, bigint and object conversion, coercion failures,
strict boolean rejection, skipped getters, and getter failure.

Each of the five controls below was measured at **259a993ab2** plus only its
documented patch. Every mutant compiled and failed at the named native runtime
assertion; every exact source restoration then passed all 87 cases.

| Control | Named native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| Remove array-result negation | `CRYPTOCOM-API-GUARD-NATIVE record case9 object` | 101 | 0 |
| Invert parsed-record acceptance | `CRYPTOCOM-API-GUARD-NATIVE parse case0 object` | 101 | 0 |
| Change authentication code | `CRYPTOCOM-API-GUARD-NATIVE code case8 number-code` | 101 | 0 |
| Invert substring match | `CRYPTOCOM-API-GUARD-NATIVE message case6 exact` | 101 | 0 |
| Accept false suppression flag | `CRYPTOCOM-API-GUARD-NATIVE suppress case0 true` | 101 | 0 |

There are no survivors among these five controls. The fixed control floor
stops at the fifth control, suppression; it does not claim exhaustive mutation
coverage. Exact patches, commands, measured SHAs/variants and raw output are
retained in mutation JSON files, `gates.json` and the corresponding logs.

## Not run

Full Crypto.com suites, other crate suites, workspace compilation/suites,
canonical parity index validation, wasm runtime tests, live exchange tests,
prior slices' controls and CI were not run. Host and wasm clippy are separate
scoped compilation/lint measurements, not wasm execution proof. Mutation
controls, host/wasm clippy and formatting were not rerun after the byte-unchanged
merge; those exits remain bound to 259a993ab2. Final-merge native, scoped check
and extractor runs are separately recorded.

## Measured gate ledgers

Exact commands, elapsed times, SHA and mutation variant are in the JSON ledgers.
All Cargo runs were sequential, scoped to `safe-cex-venue-cryptocom`, with
`--jobs 2`. Native witnesses used the supplied TypeScript module path.

### Implementation — `259a993ab27fd0aa5b50fb7ab5824258bc37bbe9`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native-baseline | 0 | [native-baseline.log](native-baseline.log) |
| record-mutation | 101 | [record-mutation.log](record-mutation.log) |
| record-restored | 0 | [record-restored.log](record-restored.log) |
| parse-mutation | 101 | [parse-mutation.log](parse-mutation.log) |
| parse-restored | 0 | [parse-restored.log](parse-restored.log) |
| code-mutation | 101 | [code-mutation.log](code-mutation.log) |
| code-restored | 0 | [code-restored.log](code-restored.log) |
| message-mutation | 101 | [message-mutation.log](message-mutation.log) |
| message-restored | 0 | [message-restored.log](message-restored.log) |
| suppress-mutation | 101 | [suppress-mutation.log](suppress-mutation.log) |
| suppress-restored | 0 | [suppress-restored.log](suppress-restored.log) |
| check | 0 | [check.log](check.log) |
| host-clippy | 0 | [host-clippy.log](host-clippy.log) |
| wasm-clippy | 0 | [wasm-clippy.log](wasm-clippy.log) |
| fmt | 0 | [fmt.log](fmt.log) |
| writer | 0 | [writer.log](writer.log) |
| extractor | 0 | [extractor.log](extractor.log) |

### Final merge — `ec163d264d0a22e1e71002cd350497c24870b2b9`

| Gate | Exit | Raw output |
| --- | --- | --- |
| native | 0 | [merge-native.log](merge-native.log) |
| check | 0 | [merge-check.log](merge-check.log) |
| writer | 0 | [merge-writer.log](merge-writer.log) |
| extractor | 0 | [merge-extractor.log](merge-extractor.log) |

Both writer outputs report Updated 0 manifests; both extractor checks report
Would update 0 manifests. Output texts were verified in addition to the measured
process exits.

## Sub-agent review

`review_auth_witness` found no blockers at exact implementation commit
`259a993ab27fd0aa5b50fb7ab5824258bc37bbe9`. Its independent commit-to-commit
comparison exited 0 for reviewed guard source, tests, oracle and API manifest
at merge `ec163d264d`. It confirmed faithful short-circuit order, scoped catch
behavior, opaque value/error identity, UTF-16 handling and strict boolean
suppression at the documented dependency boundaries. It inspected all five
exact-SHA patches and raw mutation/restoration logs, confirming compiled named
native runtime failures 101, restored passes 0 with 87 native cases, and no
survivors among the five controls. It also inspected the completed gate and
merge ledgers. The reviewer ran no Cargo. It does not claim builtin/API wiring
completion or relabel premerge clippy/control results as postmerge runs.
