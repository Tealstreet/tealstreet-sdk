# WOO v3 algo-write evidence

**43 rows moved: 2 pending → ported, 41 → divergent**, measured by tomllib against actual built-on integration parent `a07e971bb08f867925ff18ca8b13d2f288d0deba`. Fresh branch `feat/woo-v3-algo-writes`; no unrelated feature-branch ancestry. Source gate SHA `006002e1a2b291232da0bd3d9e576e89d5ed4ab7`. Later evidence-only commits do not relabel exits.

Fixed census: 22 manifests / 534 WOO+journal rows; 43 distinct notes and 50 distinct existing Rust targets. Before {'divergent': 242, 'ported': 141, 'pending': 151}; after {'divergent': 283, 'ported': 143, 'pending': 108}. Only woo.v3.algo-writes.ts statuses changed. All 43 rows now have Rust; this is a census, not complete WOO parity.

## Measured gates at the source SHA

Scoped all-target cargo check **0**; full scoped WOO all-target tests **0** (46 targets, 147 passed, 0 failed, 0 ignored); extractor **0**, output inspected for zero updates/additions/orphans; standalone source oracle **0**; leaf Rust formatting **0**; tomllib census **0**. Cargo used jobs2/offline/locked, serially. Oracle env SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.

Optional whole-WOO target audit **1**, 135 inherited diagnostics in legacy_metadata.rs and legacy_socket_control.rs, zero new algo module diagnostics. First call site: WOOXExchange#fetchAccountInfo → legacy_metadata.rs::fetch_account_info (undeclared source file and missing exact qualified rename note); host helper mappings also absent. These files are outside the assigned partition and were left intact. No whole-WOO target-audit success is claimed; complete diagnostics are in JSON.

## Discrimination

Six exact native controls at `4ebefb7b2108d87752b96d0247c077321d52734c`, stopped at iteration6, zero survivors. All six substitutions shared ONE explicitly combined overlay; six separate exact-test commands each failed its own named native assertion with101. These are not six isolated mutation measurements. Exact source bytes were restored once in finally, then each exact test passed0 (1 passed /5 filtered). Control driver0, source/test/oracle/manifest survival diff from the baseline source SHA0, and post-control restoration diff0. The only intervening committed change is the control driver.

| Family / exact test | Named assertion | Broken / restored |
| --- | --- | --- |
| declarations / native_declarations | ALGO-WRITES native declaration quantity | 101 / 0 |
| helpers / native_validators | ALGO-WRITES native helper array length | 101 / 0 |
| place / native_place_builders | ALGO-WRITES native captured string | 101 / 0 |
| amend-order / native_amend_builders | ALGO-WRITES native first amend blocker | 101 / 0 |
| cancel / native_cancel_builders | ALGO-WRITES native request endpoint | 101 / 0 |
| responses / native_response_decoders | ALGO-WRITES native gateway name | 101 / 0 |

Declaration mutation swaps quantity/triggerPrice Serde labels symmetrically. Helpers remove an array element. Placement reverses the hedge BUY side. Amend mutation physically moves terminal/trigger checks before the partial-fill check; a case with both blockers asserts which came FIRST. Cancel uses the singular pending endpoint. Responses alter the concrete gateway error name. JSON retains all simultaneous mutations, call sites, separate commands and full named-failure/restored logs. Read-only review checked that each exact test reaches its intended assertion before any other substitution can interfere. The initial serial attempt proved declaration101/restored0, then was deliberately interrupted during the second helper build (driver exit-2, shell254); no helper result is claimed from that abandoned attempt. Its receipt is retained. A read-only process sample exited255 because the sampled process had already exited; no stack or runtime proof was captured.

## Tested floors and boundaries

Fixed source oracle floors: validators105, places31, amends32, cancels9, placeRows8, placeResponses17, successes10, mutations22; runtime cases234. Also declarations14, declaration refusals9, trigger variants4, exact10metadata keys/values and concrete error constructor fields. Enumeration stopped at these counts; no arbitrary-JavaScript completeness claim.

Native fields, variants and f64 bits are inspected directly; no roundtrip equality. Signed zero/non-finite declared numbers are captured before Serde buffering; runtime IDs/timestamps enforce source safe-integer minima. Distinct nested prices/update fields expose swaps. AST pins Readonly records, intersections, exclusive target, three placement families, and all four ready/blocked unions' requiredness, exact field types and literals. Error/blocker assertions observe first result, including partially filled + terminal amendment and envelope validation order.

Each divergent row has its own CHOSEN rationale and exact Rust symbol. Adaptations include typed guards for off-signature JavaScript values, exclusive target enums, owned captured gateway trees instead of shallow aliases, copied immutable metadata instead of frozen identity, and operation-only result enums without arbitrary declaration ingress. Literal-valued public String/bool construction is broader; ingress and operation results enforce the relevant source literal. Only the trigger enum and unsupported-trigger predicate are marked ported. No live routes, shared types/traits, HOW or other lanes' files authored.

Read-only sub-agent review found no production mismatch in the declared input domain. Its initial operation-union AST proof gap was strengthened before the source commit; final review found no new blocker. Review ran no gates or edits.

Not run: workspace compilation/tests/parity audit, monorepo suites/typecheck, HOW-specific tests, host clippy, wasm clippy/build, doctests, CI reruns, live or credentialed demo lifecycle. No exits claimed for them. The manifest writer ran successfully before the source commit; committed extractor check above verifies its result.

## Completed merge-down

Measured merge `f4e7407a727f1cfbb3dd099ae3f7b0315d030073`, actual integration parent `00cafd8f46790c3e283decca55b6618bf8ea4d97`. Fetch0; initial merge1 only for shared lib.rs export append; retaining all exports and committing resolution0. No unrelated unreviewed feature branch was used as parent. Own algo source/tests/scripts/manifest survival diff0. Broader crate src/tests diff1 because integration adds legacy_market_reads and legacy_order_control plus their tests; these incoming files and legacy manifests match the actual integration parent exactly (diff0).

Fresh scoped all-target check0; affected test command0: **3 targets / 23 passed /0failed /0ignored** (algo_writes, legacy_market_reads, legacy_order_control). Full147-test WOO baseline remains attributed to006002e1a2, not this merge. New extractor0, inspected zero updates/additions/orphans. Tomllib census against actual merged parent0: **43 moved,2ported/41divergent**, pending 135 → 92; final merged WOO totals {'divergent': 283, 'ported': 159, 'pending': 92}. These figures include peers' integration work only in the totals, not this slice's delta.

Shared/local dependency sources, workspace Cargo.toml, WOO Cargo.toml and source algo/contracts diff0. Cargo.lock changes only an unrelated safe-cex-lighter-sdk record; independently traversed WOO-rooted full package records/edges are unchanged, stopped at481packages (closure check0).

Optional audit using the existing prebuilt parity-check binary exited1: **250 inherited diagnostics** across legacy_market_reads.rs, legacy_metadata.rs, legacy_order_control.rs, legacy_socket_control.rs, zero algo-module diagnostics. New first-file example: legacy_market_reads.rs public Host helpers lack manifest/exemption mappings under this audit. All implicated legacy source files/manifests are byte-identical to the actual integration parent (diff0). No audit weakening or off-partition edits. Full diagnostics/commands/exits are retained in JSON. The parity-check binary itself was not rebuilt, and this is not a workspace parity or build claim.

All other not-run exclusions above remain in force; no additional mutation matrix or full WOO postmerge rerun was performed.

## Final measured dependency merge

Merge `ce24facbf15d23f9088c835a70809e1d332ee094` includes actual integration parent `9c663a908f40c2955e27b1ce58f97fb5c08fb331`. Fetch0/merge0. Whole WOO src/tests/scripts/manifests survival diff from f4e7407 is0. An incoming shared BaseWebSocket optional before_close hook changes dependency source; it is integration work, not authored by this slice. Fresh scoped all-target check0 and selected WOO library+algo tests0: **2targets/16passed/0failed/0ignored**. No full WOO test rerun or workspace build is claimed at this merge. Prior147passes and23affectedpasses remain bound to their exact earlier SHAs.

Fresh extractor0, output inspected zero updates/additions/orphans; fresh tomllib census0 against actual parent: **43moved=2ported+41divergent**, pending135→92. Lockfile's only new record difference is unrelated bitrue dependency; WOO-rooted full481package records/edges unchanged (closure0). Inherited250-diagnostic optional prebuilt target audit remains attributed to f4e7407; it was not rerun at this dependency merge, and WOO audit inputs are byte-identical.

Read-only evidence review found no material attribution issue through f4e7407; that review does not claim to cover this later dependency merge. Runtime gates here are measured separately. All other not-run exclusions remain.

## Push merge survival

Final pre-push code merge `f729b94cc25e21d57bb0b7e43d9e1f42b8a009c2` includes actual integration parent `7daf9d4c8c6805aaff3aaee1fbc6a0bbe6eedab2` (fetch0/merge0). Scoped WOO source/tests/scripts/manifests, all local dependency source trees, Cargo files/lock, source algo/contracts, tool configuration and JS package/lock inputs are **byte-identical to ce24facbf1**, measured survival diff0. No Cargo rerun at this merge; measured ce24 compile0 and selected16passes retain their original SHA attribution and unchanged inputs. Fresh extractor0 (zero updates/additions/orphans), fresh census0: **43 rows moved,2ported/41divergent**, WOO pending135→92 against the actual parent. A subsequent evidence-only delivery commit adds this receipt; it does not relabel runtime gates. No unrelated unreviewed feature branch ancestor.
