# Bitrue websocket symbols and private open proof

Moved **3 pending → ported**, **0 → divergent**: `BitruePublicWebsocket.convertToWsSymbol`, `BitruePublicWebsocket.formatKlineTopic`, `BitruePrivateWebsocket.onOpen`. Independent census: [census.json](census.json), [merge-census.json](merge-census.json). Bitrue after this slice: {'ported': 24, 'pending': 58, 'divergent': 25}. Enumeration floor: six manifests, 107 rows, stopped at `BitruePublicWebsocket.listenOrderBook`; census is not runtime proof.

Fresh parent: `d8ab034625643795919e84c63ee6e5d764cb11df`. Code/control SHA: `ce90a737147b3a86de21f6c342db30612f7e4a40`. Latest integration merged down: `ab593ef7504191a1cfd2bb094ef066aedd4c5c25`. Post-merge measurement SHA: `9cb39ad77312e1a8bc47cb0197f4d0157ba03e5f`. The only own ancestors outside integration are this slice's implementation and witness-inventory/manifest-note correction. No unrelated unreviewed ancestor was imported. Evidence-only delivery does not relabel these measurements as gates at the delivery SHA.

All Cargo commands are scoped to `safe-cex-venue-bitrue`, sequential, jobs 2 where applicable. Raw logs and ledgers attach commands/exits to the actual measured SHA; mutation records describe the sole intentional change from code SHA. No gate still running is claimed.

## Code SHA measurements

| Gate/control | Exit | Raw log |
| --- | --- | --- |
| `native-baseline` | 0 | [native-baseline.log](native-baseline.log) |
| `convert-mutation` | 101 | [convert-mutation.log](convert-mutation.log) |
| `convert-restored` | 0 | [convert-restored.log](convert-restored.log) |
| `topic-mutation` | 101 | [topic-mutation.log](topic-mutation.log) |
| `topic-restored` | 0 | [topic-restored.log](topic-restored.log) |
| `open-mutation` | 101 | [open-mutation.log](open-mutation.log) |
| `open-restored` | 0 | [open-restored.log](open-restored.log) |
| `check` | 0 | [check.log](check.log) |
| `host-clippy` | 0 | [host-clippy.log](host-clippy.log) |
| `wasm-clippy` | 0 | [wasm-clippy.log](wasm-clippy.log) |
| `fmt` | 0 | [fmt.log](fmt.log) |
| `writer` | 0 | [writer.log](writer.log) |
| `extractor` | 0 | [extractor.log](extractor.log) |

Each of the three controls compiled and failed at runtime: convert changed `e_` to `x_` and failed `BITRUE-WS-SYMBOLS-NATIVE convert case0`; topic changed `market_` to `markets_` and failed `BITRUE-WS-SYMBOLS-NATIVE topic case0`; private open omitted ping and failed `BITRUE-WS-SYMBOLS-NATIVE-EFFECTS open case0`. Each exact restoration then passed. Mutations were run in that order and stopped after private open, index 2; three controls are a floor, not exhaustive mutation coverage. No survivor in that floor.

## Post-merge measurements

| Gate/comparison | Exit | Raw log |
| --- | --- | --- |
| `source-unchanged` | 0 | [merge-source-unchanged.log](merge-source-unchanged.log) |
| `inputs-comparison` | 0 | [merge-inputs-comparison.log](merge-inputs-comparison.log) |
| `lock-comparison` | 0 | [merge-lock-comparison.log](merge-lock-comparison.log) |
| `native` | 0 | [merge-native.log](merge-native.log) |
| `check` | 0 | [merge-check.log](merge-check.log) |
| `writer` | 0 | [merge-writer.log](merge-writer.log) |
| `extractor` | 0 | [merge-extractor.log](merge-extractor.log) |

The source comparison includes the entire venue src/tests tree. Dependency comparisons include venue TypeScript, Cargo manifests and direct shared num/exchange/store/types sources and manifests (additional-comparisons.json verifies each path exists); the lockfile comparison is separate. Host/wasm clippy and fmt are only claimed at the SHA in their raw logs; an unchanged input comparison is not a fresh rerun. Extractor success was checked from text (zero updated manifests, zero added/orphaned), as well as measured process exits.

## Native proof and limits

The actual complete TypeScript arrow initializers are transpiled and invoked by the source oracle. Independent fixture expectations are 20 converter cases, 24 topic cases and 12 private-open cases: 56 native cases, stopped at open case11 `promise`. Rust asserts decoded UTF-16 strings, native void, original generic error carrier identity and effect order before inventories/ingress checks. TypeScript asserts original thrown object identity before emitting its records. No decode/re-encode roundtrip is used.

Converter implements both ASCII regex operations over UTF-16 and the prefix; full JavaScript Unicode lowercase is an explicit dependency. Topic implements ordered invocation and template concatenation at explicit conversion and interval lookup/template boundaries, including generic interval identity and unknown/prototype keys. Private open implements the synchronous guard-once/subscribe/ping body, including throwing lookup/call boundaries, disposal changes during subscribe and ignored promise returns. Dependency event segments from JavaScript are supplied to Rust callback stubs; these do not prove native implementations of Unicode lowercase, property lookup, template coercion, INTERVAL data or websocket class/transport/caller wiring. Those remain separately pending.

Read-only sub-agent review of the implementation at `3148de9e29` found no body/witness blocker and required exact qualified standalone Rust target tokens in notes. The correction at `ce90a737147b3a86de21f6c342db30612f7e4a40` also fixes an inventory-loop clippy warning. Both the correction and all controls/gates were rerun at their own SHA; historical failure measurements are retained in ../ws-symbols-open-3148de9e29 and are not relabeled. Correction re-review found no blockers; see [subagent-review.md](subagent-review.md). No human review or Cargo run by the reviewer.

The first uncommitted development test failed exit 101 because the global lowercase hook also captured TypeScript compiler activity; building the source converter before installing the hook fixed instrumentation, and that development rerun passed exit 0. Both diagnostic logs are retained and explicitly excluded from mutation proof.

Not run: complete Bitrue test suite, Crypto.com tests/checks, workspace/repository builds or suites, canonical index gate, wasm runtime, live venue/network behavior, CI or prior slices' controls. This is no completeness claim beyond the named row bodies, fixture and control floors.
