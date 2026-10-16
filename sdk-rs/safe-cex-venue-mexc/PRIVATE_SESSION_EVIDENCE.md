# MEXC private websocket session bodies

Eight rows **pending → ported**, zero to divergent. Independent tomllib census: **110 / 40 / 35 → 118 / 40 / 27**, 185 rows in six manifests. [Census](evidence/private-session/census.json) enumerates the fixed eight symbols, stopping at sixth file `mexc.ws-public.ts.toml`; this is separate from behavior and target proof.

Branch `feat/mexc-private-session-ad731b`. Actual original parent **ad731bb9ef4c5c6c8255645a3b412e933f8acda2**, directly on integration; no unreviewed lane branch ancestor. Implementation **bd997e826b720a8a6f2f73c9ca9e834729696461**. Integration merged down before gates at **1c214dccb54fe5a830ba216f93e3ed17596c38db**, including **a9268fab057cad3e028622f1817d0f834980b07d**. That merge SHA is the measured gate SHA. User reassigned future MEXC work while these gates were running; this finishes only the already committed eight-row slice.

| Gate at measured SHA | Exit/result |
| --- | --- |
| Scoped MEXC cargo check, all targets, locked/jobs2 | 0 |
| Scoped MEXC cargo test, locked/jobs2 | 0; 140 passed, zero failed; zero doc tests |
| Scoped MEXC fmt check | 0 |
| Standalone original TypeScript source oracle | 0; 98 fixtures |
| Official MEXC-only invention/status checker | 0; 185 rows, 305 indexed public Rust items, zero exemptions |
| Actual Yarn parity:extract:check | 0; output inspected: would update0 manifests, zero added/orphaned symbols |
| Compiled mutation driver | 0; 13 controls, baseline0 / named runtime101 / exact restored0 each |

[Commands/exits](evidence/private-session/gates.json), [control summary](evidence/private-session/controls/summary.json), raw logs and source corpus are tracked alongside. Cargo invocations were sequential. All oracle calls used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. The 72 new indexed declarations are native receiver/runtime/record/operation boundaries used by the eight callable bodies; the official target checker resolves every off-pending row, with no fabricated targets or exemptions.

Original arrow bodies execute through source AST extraction with lexical receiver; body text is not rewritten. Native assertions inspect decoded/constructed values directly: field names, original identity/callee captures, enum words, UTF-16 including lone surrogates, Number bits including negative zero, partial state after failures, original errors, loaded timer values and source evaluation order. Fixed fixture role schemas and event counts precede comparisons to source output. Every native assertion runs before the separate formatter/login/JSON output pass. Redaction return kind is native-asserted before its output units. No decode/re-encode or roundtrip-only proof is used.

The session functions are isolated callable method ports. Full MexcExchange socket construction/registration, legacy protocol replacement and autonomous lifecycle remain existing FINDING rows. Runtime JSON.parse, stringify, spread, crypto, signer, timer and child calls are declared native bindings. The fixture JSON parser supplies independently prepared native DOMs for fixed input JSON; this is not a general JSON parser implementation. Local signing is an observed child boundary; actual crypto is not newly proved here. General JS/proxy semantics or full classes are not claimed.

## Controls and proof floors

| Isolated production fault | Named native assertion | Baseline / mutation / restored |
| --- | --- | --- |
| connectAndSubscribe | `native private prefix count connectAndSubscribe/2` | 0 / 101 / 0 |
| onOpen | `native private prefix count onOpen/0` | 0 / 101 / 0 |
| sendLogin | `native boolean sendLogin/0/trace/event5/redactAuth` | 0 / 101 / 0 |
| createLoginSignature | `native word createLoginSignature/1/prefix/event4/type` | 0 / 101 / 0 |
| sendWithDiagnostics | `native private prefix count sendWithDiagnostics/0` | 0 / 101 / 0 |
| emitReceiveDiagnostics | `native private formatter success emitReceiveDiagnostics/0/8` | 0 / 101 / 0 |
| redactAuthFrame | `native UTF-16 redactAuthFrame/0/prefix/event4/value.signature` | 0 / 101 / 0 |
| throttledFetchBalance | `native integer throttledFetchBalance/0/prefix/event2/period` | 0 / 101 / 0 |
| boundary.taskOwnership | `native private prefix count sendLogin/10` | 0 / 101 / 0 |
| boundary.readyAwait | `native private prefix count sendLogin/9` | 0 / 101 / 0 |
| boundary.literalApiKey | `native private prefix count redactAuthFrame/12` | 0 / 101 / 0 |
| boundary.literalSignature | `native private prefix count redactAuthFrame/12` | 0 / 101 / 0 |
| boundary.returnResolution | `native private total count createLoginSignature/12` | 0 / 101 / 0 |

Each mutation compiled and reached its named failed assertion. No survivor or wrong failure in this 13-control floor; enumeration stops after `boundary.returnResolution`. The driver restores original source in finally; final byte equality checked. Reviewers stopped reading before mutations.

Source floor: **8 × 12 = 96 regular roots**, stops at fixture11 per family; two additional fixture12 witnesses give **98** roots, whole source corpus stops at fixture97, createLoginSignature/12. Native floor: same98 plus three dropped-root and three already-ready replays gives **104 root invocations**. This is a floor, not exhaustive input coverage. Native roots have four deliberately unobserved results, source roots two; their continuations/state are asserted without claiming their unobserved result values. Source timer callbacks run30 invocations, native36 due extra replays; each successful registration replays three callbacks through index2. Diagnostics formatters replay ten values through index9. Four source/native drain turns stop at turn3. Concurrent Rust test order is not a global execution-order claim.

The inherited-setter witness starts with a two-field param, then independently expects four fields after own-property apiKey/signature definitions; setter-invoking assignment faults are caught. Returned signature thenable getter/invocation/adoption are observed. Whole-method ownership survives dropped observers; already-ready children still await a source microtask turn. Create/send/error callee captures precede source argument getter effects.

## Development and review

Earlier uncommitted runs at the original parent are development diagnostics, not gates covering the final SHA. Initial scoped check0. Initial source oracle1: an unused deferred rejection was released for an unrelated synchronous family; only used deferred operations are now released. Corrected oracle2 exited0. First compiled native test run101: eight passed, connect/send-diagnostics fixtures failed due missing fixed url role and alternating Symbol-redaction expectation. Harness corrected; oracle3 exited0 with98 fixtures and corrected native run0 with10 tests. Final committed gates supersede these. Raw retained files are in [development](evidence/private-session/development); the empty first oracle stdout was not copied. Manifest edited via parseManifest/serializeManifest then actual parity:extract writer0, zero added/orphaned symbols; no hand-spelled manifest arrays. [Static review](evidence/private-session/review.md) executed no gates.

## Merge preservation and limits

Final pre-push merge includes integration **97d0e03c6f938c7e53d7ecb97e7518084267e871** at **b5817ca97fadd76b92813552513ce401ff709789**. [Preservation receipt](evidence/private-session/merge-preservation.json): own src/tests diff0; extended29 paths diff0 (scripts, manifests, TS source trees, seven dependency source/config pairs, Cargo.lock/config and checker inputs); independently expected zero MEXC exemptions remain zero. Package.json raw diff1 is solely the unrelated Kucoin oracle gaining `--test public_streams`; strict parsed equality after removing exactly that script passes0. Earlier receipt attempts failed before diff on an incorrect path slice, and then on this raw package diff; neither was claimed a successful gate. No fresh scoped Cargo/control exit is claimed at the merge head. A fresh [global extractor](evidence/private-session/merge-extractor.json) there exited0 with would update0 manifests and zero added/orphaned symbols. The evidence-only commit changes no gate inputs.

Not run: workspace compile/tests, full monorepo suites, host clippy, wasm clippy, CI rerun, Polymarket gates, live network, full production websocket composition, general JSON/runtime or new crypto proof. Remaining27 MEXC pending rows stay pending and are now the other lane's area.
