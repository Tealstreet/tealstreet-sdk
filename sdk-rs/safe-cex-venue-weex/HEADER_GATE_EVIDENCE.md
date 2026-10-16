# WEEX header and rate-limit helpers

Measured gate revision: `6f6b771cbcaa29780a662a777784d36a0c7c90fb`.
Authoring parent: `55d8addaf74e5ff2c2da80ab19a03109b6fb9476`.
The measured revision merges integration
`e5bae834bdc27615e4986fb1f3d71a8fcbb57356` into authoring revision
`9b083a6b55c5e3888d7769ebe71360711ec48351`.
No unrelated unpublished lane work was used as an ancestor.
This narrative is a documentation-only descendant of the measured revision.

## Measured gain

Three pending to ported; zero to divergent:

| Source symbol | Rust implementation |
| --- | --- |
| getHeader | headers.rs::get_header, headers.rs::WeexHeaderRuntime, headers.rs::WeexHeaderValueKind |
| getHeaderEntries | headers.rs::get_header_entries |
| getWeexRateLimitRetryDelayMs | headers.rs::get_weex_rate_limit_retry_delay_ms |

Independent Python tomllib comparison read committed rows with git show and
required exactly that fixed list of three pending-to-ported transitions:
exit 0 at the measured revision against the merged integration parent.
Baseline: 35 ported / 26 divergent / 95 pending. Slice: 38 / 26 / 92.
Both comprise 156 WEEX rows.

Rust implements the local decisions over an explicit source runtime capability
record. Generic values retain every typeof kind, original UTF16 strings,
null/undefined distinction and object/function reference identity. Property
lookup, inherited members, executable getters, receiver-bound calls, Unicode
casing, Object.entries, Number, Date.parse/clock and spread Math.max remain
host primitives. They are not represented as a narrowed JSON transport value,
nor claimed as a native JavaScript engine implementation. Caller projects
error.response?.headers; arbitrary Axios object/getter runtime remains outside
the local delay-calculator claim.

Header serialization borrows the original object and owns only an actual
toJSON result. It requires no Clone bound on the host value; the test value
deliberately does not implement Clone. Host failures propagate as Result errors.
The maximum capability can report the host's spread argument-limit failure;
the production algorithm does not replace that operation with a silent fold.

## Gates at the measured revision

All cargo commands ran serially. Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Observed exit/result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/header-mutations.py | 0; three compiled failures, each individually restored and passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; canonical invention/status gates passed: 156 rows, 73 public Rust items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols, zero updates/added/orphaned |

Extractor writer ran after the manifest edit before authoring commit
`2e5514446001702affb40b0f1b0538f1cb18be11`: one normalized manifest, zero
added/orphaned. Earlier successful draft and authoring gates are not used as
proof for the final borrow correction or changed integration dependencies;
the gates above were measured again at the exact merged revision.

## Native assertions and controls

The actual nine source declarations/constants are extracted verbatim from
weex.api.ts by headers-oracle.cjs. Its fixture model supplies primitive object
operations and their inputs; it does not replay complete helper results into
the Rust implementation. Rust selects properties, invokes methods, chooses
serialization and computes delays. Returned native kinds, f64 bits, booleans,
UTF16 strings, reference identities, ordered entry keys/values and errors are
asserted before capability traces. There is no wire roundtrip assertion.

The first corpus iteration stops at 14 lookups, 9 entry cases and 18 delay
cases: 41 results and 5 errors. Ordered identities and independent counts are
asserted. Native capability totals independently require 18 clock calls,
4 Date.parse calls, 11 maximum calls, 9 method calls, 19 entry enumerations
and 2 executable numeric coercions. This enumeration is a coverage floor.

The mutation script runs the affected target:
`cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2
--test headers`, named test `headers_match_actual_typescript`.
Each selected production mutation compiled and exited 101 at the named native
assertion, before trace comparison. Original bytes were restored after each
mutation and that test target then passed with exit 0.

| Mutation | Failed native assertion | Restored exit |
| --- | --- | --- |
| Lowercase lookup fallback changed to uppercase | headers::header::lookup_mixed::value | 0 |
| Serialized object guard changed to function | headers::entries::plain::len | 0 |
| Reverse exhausted/observed interval selection | headers::delay::exhausted_priority::value | 0 |

Three selected controls caught; zero survivors in this iteration. Other
mutations were not enumerated. Compiler failures do not satisfy the script.

Sub-agent weex_review inspected production, oracle, native assertions, controls
and manifest scopes, plus the borrow correction, and found no concrete defect.
It ran no cargo/tests/mutations and made no edits or commits. Coverage gaps:
the fixture does not change a method between its two property reads, nor
exercise a throwing maximum capability. Double reads are traced, and maximum
errors are expressible and propagated, but those two scenarios were not run.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live exchange calls, CI reruns or argument-count stress tests.
No income pager was added. Local logs are ignored under
sdk-rs/target/weex-header-gates/; this tracked sheet records their exits and SHA.

## Delivery merge

Immediately before delivery, fetched and merged integration
`cd7371a9b93dd166cfe6fcb5163f6a5439640dc5` into merge revision
`751b10d1d81747fc9c2b5d299965338713811eff`. Python required git diff
--exit-code from measured revision `6f6b771cbcaa29780a662a777784d36a0c7c90fb`
to this merge to exit 0 across WEEX src/tests/Cargo.toml/manifests, shared Rust
dependencies, Cargo.toml/Cargo.lock, parity exemptions/checker/extractor code,
package.json and both extractor source roots. It exited 0; those inputs are
byte-unchanged. Other venues' manifests changed with integration.

The measured gate exits retain their exact original SHA binding; tests and
controls were not rerun at this delivery merge. Independent TOML comparison
against the fetched integration again matched exactly the three expected
pending-to-ported rows, exit 0; counts remain 38 / 26 / 92. This evidence-only
descendant adds no source/test change.
