# WOO V3 acknowledgement, subscription and sequence gates

Branch: feat/woo-v3-ws-state. Fresh base: 9d6f40b96faafcfa73e19ffcfb2825e4d3e78444 (9d6f40b96f).
Actual integration merge-down parent: d8ab034625643795919e84c63ee6e5d764cb11df (d8ab034625), merged at
1244d98ec3be02290882f0b7c84b50ad0dffd61e before the final native timestamp fix.
Measured final source/test/manifest SHA: **dfd576610ababf8ee1d0b1268b0cf519a64d9fb8**.
The following exits were measured at that source SHA. This sheet/JSON are added by a later evidence-only
commit; no gate is claimed as executed at that documentation commit. No unrelated unreviewed lane branch
was used as an ancestor; our source commits remain subject to the user's PR review.

## Actual completed gates

| Gate | Exit at dfd576610ababf8ee1d0b1268b0cf519a64d9fb8 |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | 0; 37 observed targets / 95 passed / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; output: 7341 symbols across 512 files, Would update 0 manifests, 0 added, 0 orphaned |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows, 276 indexed public Rust items |
| rustfmt --edition 2024 --check src/v3_ws_state.rs tests/ws_state.rs | 0 |
| python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-state-controls.py | 0; four compiling named failures and four restored passes |
| git diff --exit-code after controls | 0; exact source restoration before adding evidence |

Cargo ran serially. Tests/controls used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Extractor output text was inspected rather than trusting only yarn's process exit. parity:extract was
used as the manifest writer after every mapping/note edit; rust_files was derived from actual rs targets
using tomllib. The target audit indexes WOO with 22 selected WOO/journal manifests and sibling dependency
symlinks; it is not a full-workspace parity audit.

## Initial failed gate and correction

At 1244d98ec3be02290882f0b7c84b50ad0dffd61e the scoped check/audit/extractor/fmt exited 0, but full WOO tests
exited **101**. native_ack_validation_and_sequence_match_source reached the expected native sequence
path and found `invalid type: floating point 0.0, expected JSON numeric input`. A primitive f64 Serde
adapter did not implement retained JSON's required newtype capture protocol. This was a runtime defect,
not a successful discrimination control. The complete initial gate receipts and failure excerpt remain
in WS_STATE_GATES.json, anchored to the initial SHA.

Fix dfd576610ababf8ee1d0b1268b0cf519a64d9fb8 applies the source safe-integer/minimum-zero predicate directly to the already-native f64
expected timestamp. It retains bits without a JSON wrapper/conversion. Only expected is validated;
update previous/next numbers remain unchecked as in source. The fix was independently reviewed and all
final gates were rerun at the fix SHA. The earlier passing check does not cover the fix.

## Decoded native evidence and finite floors

The oracle executes the whole actual TypeScript contracts/WebSocket modules. It independently pins
eight blocker literal codes, three record fieldsets/requiredness/types, readonly string-array fields,
blocker requirement/ack-success literals, and both sequence branch fieldsets/types/literals.
Native number values are observed before JSON projection; raw fixture ingress retains -0/infinity.
All new tests assert native enum variants, record fields, strings/vectors and numeric bits directly,
before any wire assertion; no Rust decode/re-encode comparisons occur in this target.

Fixed walks stop at blocker entry 8, acknowledgement case 31, sequence case 9, create case 12, plan case
12, acknowledgement-state transition 9 and reconnect case 6. These floors do not imply exhaustive
runtime or mutation coverage. Plans use independent fixture expectations for 0/1/20/21/41/100 topic
counts and every generated native ID/channel/command/topic slice. Generation remains f64, including
2^53 rounding and infinity. Sequence results retain separate expected/actual fields and next timestamp,
including -0 and unchecked infinity. Both result variants are decoded and checked natively.

Multiply-invalid acknowledgement fixtures pin first-error order: id before command/success, command
before failure envelope, element typing before duplicates, parsing/channel homogeneity before time,
time before expected identity, then identity/channel/exact original topic order. Nonempty data is paired
with empty expected params (refusal) and absent params (success). Subscription state transitions check
channel and SUBSCRIBE command kind before malformed acknowledgement, refuse unexpected intent, and
filter accumulated membership into intended-topic order, retaining duplicates in directly constructed
intent. Gap requiresFreshSnapshot is compared with the actual source value and independently pinned true.
Source frozen objects/arrays and reconnect intendedTopics alias identity are observed separately from
native ownership; those are disclosed adaptations, not native alias claims.

## One discriminating mutation per new family

| Family | Compiling production mutation | Named failed native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- | --- |
| Blocker contracts | failure code parser returns request-ID enum variant | WS-STATE native blocker code | 101 | 0 |
| Acknowledgements | decoded time adds one | WS-STATE native ack time | 101 | 0 |
| Subscription state | reconnect generation adds two | WS-STATE native generation | 101 | 0 |
| Sequencing | accepted next uses previous timestamp | WS-STATE native sequence next | 101 | 0 |

Each control ran as an explicit overlay on dfd576610ababf8ee1d0b1268b0cf519a64d9fb8, compiled and failed its named native assertion;
exact saved bytes were restored before every three-test ws_state pass. Stopped after mutation iteration
4 with zero survivors in that floor; no further matrix or per-symbol discrimination enumeration is
claimed. Commands, patches, call sites, assertions and excerpts are retained in WS_STATE_GATES.json.

The native sub-agent reviewed source order, typed number semantics, actual targets and control anchors.
It identified three proof gaps (absent/empty params, source snapshot bool and declaration AST types),
which were fixed before committing. It later confirmed the native timestamp predicate fix without
running gates. Its original source oracle run exited 0; no Rust gate/control/audit is attributed to it.

## Independent census and boundaries

Commands: python3.14 sdk-rs/safe-cex-venue-woo/scripts/ws-state-census.py d8ab034625643795919e84c63ee6e5d764cb11df,
and the same script against 9d6f40b96faafcfa73e19ffcfb2825e4d3e78444.
Python tomllib reads all 22 selected WOO/journal manifests, independently asserting the known 534-row
inventory and a separately enumerated thirteen-symbol expectation. Fresh base and integration parent
have the same WOO counts.
**13 pending rows moved: 1 ported / 12 divergent.**
Before: 102 ported / 96 divergent / 336 pending. After: 103 / 108 / 323.
All thirteen notes are distinct and name their own real v3_ws_state.rs target, including supporting
literal codecs/record declarations. The exact blocker-code string union is ported. Every divergent
row has a canonical CHOSEN explanation specific to its adaptation; all real targets passed indexing.
The JSON preserves changed rows, targets and every note.

Chosen boundaries include frozen/Readonly source objects versus owned mutable native records/vectors,
a typed fixed blocker array versus source string-keyed object identity, static reference promotion
versus source blocker alias identity, Cow reason ownership, Result messages versus Error identity,
and retained dense UTF-8 JSON versus executable/accessor/prototype/sparse/lone-surrogate values.
Public bool/String constructor fields deliberately widen acknowledgement/sequence literals while
map-only declaration decoding enforces them. Native typed state preserves declared fields; arbitrary
extra state properties from JavaScript object spread are not modeled. Borrowed state/command and
validated topic snapshots exclude live alias/accessor changes; reconnect copies intended topics where
source reuses an array alias. These primitives do not create credentials, timers, live sockets or
automatic subscription transport. Private payload/frame rows remain pending; no HOW row moved.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for those.
