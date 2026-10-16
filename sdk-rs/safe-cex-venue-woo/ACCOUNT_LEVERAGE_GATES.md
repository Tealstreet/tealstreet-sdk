# WOO V3 account leverage gates

Branch feat/woo-v3-account-writes started fresh from **77746a5b498ce3d3532424345f072425181f33ca**.
Actual built/gated source/test/manifest SHA: **0a9b1e014fbb01624453a258719af28d3362350f**.
No unrelated unreviewed branch was used as an ancestor; this slice's source is the only new feature work.
Latest integration merge-down parent **91bdde133d2f407dffa8f2140f94e87eac7c1d5f**, merged at **5a5e502bd76282778d98f3e3dfdfb4b29b7306a8**.
Scoped source/test/manifest comparison from the source SHA and script comparison from
daba14876ab0fd74a0764b7185fc2d7ab8f46d4a each exited **0**. Direct dependency inputs remained byte-identical except Cargo.lock: an unrelated OKX entry added
indexmap/safe-cex-crypto. A parent-versus-merge lockfile graph comparison rooted at WOO exited 0,
stopping at 481 reachable package records; all identities, dependency edges and full records matched. Source gate exits stand for the unchanged production/tests. The control-runner-only fix at
ce51436255c7411fd8b938ac203e372dfe216c78 preserved mode_text usage under deny-warnings and added a
start-family option. Two controls succeeded there before the readback replacement caused an
unused-variable compile diagnostic. A second script-only fix at daba14876ab0fd74a0764b7185fc2d7ab8f46d4a
keeps the LONG binding in use; its four resumed controls and restoration exited 0. This sheet/JSON are added
by a later evidence-only commit; no source gate is claimed as executed at that documentation/merge SHA.
Post-merge extractor and tomllib census each exited 0 at the merge SHA, with no drift and the same
25 moves against the actual integration parent. Those separate SHA-bound receipts are in the JSON.

## Completed measured source gates

| Gate (source SHA unless otherwise stated) | Exit and observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | 0; 40 targets / 108 passed / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 files, Would update 0 manifests / 0 added / 0 orphaned |
| python3.14 scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows / 340 public Rust items |
| rustfmt --check --edition 2024 src/v3_account_writes.rs tests/account_leverage.rs | 0 |
| node scripts/account-leverage-oracle.cjs | 0; actual whole TS account/contracts modules and declaration AST checks |
| python3.14 scripts/account-leverage-census.py 77746a5b49 | 0; 25 independent pending-row expectations matched |
| initial controls at source SHA | runner 1 after two valid controls; third failed compilation (dead-code), not native discrimination |
| resumed controls --start-at get-descriptor at ce51436255c7411fd8b938ac203e372dfe216c78 | runner 1 after two valid controls; readback replacement failed compilation (unused variable) |
| resumed controls --start-at readback at daba14876ab0fd74a0764b7185fc2d7ab8f46d4a | 0; four named failures and four restored passes |
| git diff --exit-code after controls | 0; exact tracked source restoration |

Cargo used /opt/homebrew/opt/rustup/bin/cargo, one invocation at a time with jobs 2. Oracle tests and
controls used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Extractor
output text was inspected. parity:extract was the manifest writer, and rust_files was derived from
actual targets. Target indexing scopes 22 WOO/journal manifests and sibling dependencies; this is not
workspace compilation/audit. Full commands, SHA anchors, patches, call sites and failed/restored
assertion excerpts are recorded in ACCOUNT_LEVERAGE_GATES.json.

## Native evidence and fixed floors

The oracle executes the actual whole account-writes module and contracts import. It independently
pins both two-value mode unions, four Readonly record schemas' required field names/types/literals/
references, and both readback branches' required fields, mode literals, readonly one/two-tuples,
nested Readonly detail records and exact BOTH/LONG/SHORT literals/numeric fields. Native assertions
inspect each enum variant, declaration field, request method/path/query/body entry, target/mode/leverages,
blocker/scope, readback leg/time, mutation acceptance/state evidence and concrete error fields. Every
captured gateway/query/body value is recursively decoded and checked, including nested numeric bits,
before any wire assertion. No Rust decode/re-encode equality appears in this target.

Fixed walks stop at helper case 90 (five independently named families, eighteen kinds each), exact-key
case 5, symbol case 8, provenance case 9, GET build case 6, SET build case 13, readback case 29,
success case 12, mutation case 14 and standalone declaration case 6. Each mode union stops at value 2;
five separately enumerated invalid JSON kinds/literals are refused by both enum decoders. Metadata
inspection stops at field 11. These are finite floors, not exhaustive JavaScript or branch enumeration.
Declaration fixtures retain infinity and -0 through numeric input and tagged readback graphs;
nested request/provenance fields are independently inspected. Runtime predicates require safe integers
and minimum leverage one. Hedge inputs use
LONG=17 and SHORT=29 in both orders, ensuring fields are independently identifiable before canonical
LONG/SHORT return ordering. Exact-key tests include a failing astral/BMP UTF-16 sort-order witness.

Multiply-invalid cases pin symbol before margin/position; symbol/margin/leverage before provenance;
provenance source before ID/account/position; matching request method/path before response; envelope
keys before timestamp/data; timestamp before data; scope before detail container; detail keys before
side/leverage; and all detail validation before leg cardinality. Vendor failure code precedes message.
Default message/code absence and concrete name/contract status/gateway own-key order are asserted.
Set blockers retain SPOT before ISOLATED before HEDGE precedence. Accepted setters report request
intent only; no resulting account/leg state is inferred from success.

## One compiling control per new family

| Family | Named failed native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| declaration-contracts | ACCOUNT-LEVERAGE native position mode | 101 | 0 |
| validators | ACCOUNT-LEVERAGE native integer | 101 | 0 |
| get-descriptor | ACCOUNT-LEVERAGE native captured string | 101 | 0 |
| set-descriptor | ACCOUNT-LEVERAGE native set build readiness | 101 | 0 |
| readback | ACCOUNT-LEVERAGE native detail leverage | 101 | 0 |
| mutation | ACCOUNT-LEVERAGE native mutation timestamp | 101 | 0 |
| gateway-error | ACCOUNT-LEVERAGE native gateway code | 101 | 0 |
| metadata | ACCOUNT-LEVERAGE native runtime imported | 101 | 0 |

First two controls overlay 0a9b1e014fbb01624453a258719af28d3362350f, GET/SET controls overlay ce51436255c7411fd8b938ac203e372dfe216c78,
and the remaining four overlay daba14876ab0fd74a0764b7185fc2d7ab8f46d4a. Production and tests are
byte-identical at these three commits: symmetrically swap mode serde labels; erase -0 sign with
abs(); substitute ONE_WAY in the GET query; reverse the MARGIN readiness predicate; use SHORT leverage
for LONG; add one to accepted timestamp; add one to gateway code; enable the runtimeImported metadata
flag. Every mutation compiled and failed its named native assertion, then restored saved bytes and
passed all four account_leverage tests. The initial GET query control caused a deny-warnings
compile error by eliminating the sole mode_text call (cargo101, runner1); this is explicitly
not a native failure or a survivor. Its corrected constant-mode argument retains the call.
The first readback replacement also failed compilation by leaving LONG unused (cargo101, runner1);
it now retains that read while returning SHORT leverage for LONG. Neither compile-only diagnostic
is a native failure or a survivor. All three measured phases and both aborted diagnostics are
preserved separately in the JSON. Stopped at mutation iteration **8**, zero survivors in that
floor; no further matrix is claimed. Symmetric enum encoding/decoding still roundtrips, so only the
native variant assertion establishes that control's discrimination.

The native review sub-agent found no production mismatch in the stated domain and checked all 30
primary/supporting targets, 25 distinct notes and the original eight unique anchors. GET and readback replacements needed the compiling fixes
described above, which the original static review did not detect. It found declaration AST
checks omitted field types/nested detail literals; these checks were strengthened before the source
commit and gates. The reviewer ran no cargo/control/extractor/audit gate; none is attributed to it.
During precommit oracle preparation, an extra AST type link caused a TypeError (node exit1); it was
corrected before source commit. This preparation diagnostic is not a native discrimination result.

## Independent census and deliberate boundaries

Tomllib reads all 22 WOO/journal manifests, asserts 534 known rows and an independently enumerated
25-symbol expectation. **25 pending rows moved: 2 ported / 23 divergent**.
Before: 107 ported / 141 divergent / 286 pending. After: **109 / 164 / 261**.
All 25 notes are distinct and name their primary target; five supporting return/target/error/metadata
types are explicitly mapped. Both exact mode string unions are ported. Each divergent note gives a
specific canonical CHOSEN adaptation, including aliases, literal constructor width or input domain.
Eight position-mode evidence/builder/decoder rows in this file stay pending and have no invented target.

Builders take captured object maps and string-only symbols. Source property-access TypeErrors and
executable/prototype/accessor input behavior are outside this interface. The source regex can coerce
an untyped single-element symbol array and return an array target; that source acceptance and native
string-contract refusal are asserted separately as a deliberate adaptation, not silently equated.
Readonly/frozen source records and query-target identity become separately owned snapshots. Strings
use UTF-8, excluding lone UTF-16 surrogates. Runtime/readback arrays and declaration record shapes
remain checked; wider direct native String/bool/target construction is disclosed per affected row.
BuildResult and MutationResult are operation-output models without standalone Serde declaration ingress;
no such ingress parity is claimed. MutationTarget includes the declared PositionMode variant, but
that pending setter path is not enumerated. Native gateway errors preserve concrete data via a full
retained snapshot rather than source Error inheritance/stack and shallow spread's nested aliases.
Both source nested alias behavior and native snapshot stability after original input replacement
are observed. Metadata retains runtime/demo false and every unresolved scope exactly. No shared type
or trait changed, no live transport is enabled and no HOW row moved.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for these.

Final read-only sub-agent review found no material issues and independently checked the logs,
25 mappings, split control SHAs, preserved diagnostics and 481 unchanged dependency records. No
gates were rerun or files edited by the reviewer. Final fetch at 40ce3558cd1896c14d91d49ae96af36bdee945ce exited 0; integration
still pointed to 91bdde133d2f407dffa8f2140f94e87eac7c1d5f, merge reported already up to date (0), and both source/script
survival comparisons again exited 0. This supplement is a documentation-only commit.
