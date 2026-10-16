# Command handler gate evidence

Gate/control SHA: `3218911c120712a973bf35fa3730e9ae64176a23`. Implementation commit: `9b618e4ed7`; native replay/readiness witnesses: `22c7a6692c`; final per-symbol note correction: the gate SHA. Actual initial parent: `976c7bf08eb0d23329d7dfd9042035e3aabe2358` (reviewed integration). Branch: `feat/rust-webworker-command-handlers`. No external unreviewed branch is an ancestor; the three own slice commits were reviewed by the sub-agent.

## Rows moved

**0 pending -> ported, 6 pending -> divergent (MIXED)**: ChaserOrderHandler, TwapHandler, UiStateHandler and each handle method. All targets are actual classes/methods in `src/command_handlers.rs`. Shared String contracts are FORCED; caller-owned live context, typed payload validation, composed BaseHandler/Weak replay and native task ownership are CHOSEN for linking/ownership without duplicating a JS object/Promise engine. Handler-specific differences are in their individual rows. Existing helper mappings/statuses remain; one stale note saying the UI class was pending was corrected.

Python 3.12 tomllib independently compared the fixed six symbol identities against initial-parent manifests. Census stopped at **file 38 / row 222**, checked against the known 38-file/222-row floor: **112 ported / 74 divergent / 36 pending -> 112 / 80 / 30**. This census is separate from semantic proof. Unimplemented chaser-manager cancellation/fill reconciliation remain pending; required host methods do not manufacture partial Order values or return default-empty fills.

## Measured gates

All measured at `3218911c120712a973bf35fa3730e9ae64176a23` with restored tracked source:

| Gate | Exit | Measured output |
| --- | --- | --- |
| Scoped cargo check, all targets | 0 | Finished dev profile |
| Command-handler native/oracle target | 0 | 4 passed, 0 failed |
| Extractor check | 0 | 7341 symbols / 512 source files; would update 0 manifests, 0 added, 0 orphaned |

```sh
cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test command_handlers --locked --jobs 2
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

Manifest edits used parseManifest/serializeManifest and the writer parity:extract, three runs each exit 0 with inspected text `Updated 0 manifests; 0 added, 0 orphaned symbols.` Writer exits alone were not taken as success. Earlier committed gates at `22c7a6692c` also exited 0; they are not substituted for the final gate table.

## Native assertions and source floor

Every native assertion runs before its source profile comparison; no decode/re-encode or roundtrip gate. Real native ChaserOrderInstance/TwapInstance, BaseHandler, BaseExchange and DefaultStore are invoked. Direct native witnesses pin context Rc identity, three independent timeout/class names, successful Weak replay through all three handlers, actual decoded chaser fields/IDs, live account fallback, exact termination routes/reason, TWAP state transitions and order amount, add-before-start, stale binding no-op, restore duplicate skipping/catch cutover, original supplied status after add replaces current state, both pause/resume rejected futures, raw empty IDs before generated-ID add, UI active-symbol ordering/live metadata, pending placeholder before return, fulfilled native position leverage, and symbol throw preventing margin work.

Actual TS three handlers/BaseHandler/TwapInstance/TwapManager are loaded, with source enums/safe-math. Source ChaserOrderInstance and chaser-manager bindings are fixtures; venue/store/worker/timers are also dependency fixtures. This gate does not execute the TS Chaser lifecycle or full production TS Store. Native constructor values are asserted separately, rather than claiming mock dependencies prove those implementations. Source profiles deliberately compare selected handler effects, not every lifecycle event or metadata field. The class loop stops at iteration 3 (UI), account loop at iteration 4 (explicit account), command loop at iteration 4 (stop); restore and UI scenarios are finite named witnesses, not an exhaustive dynamic-record or scheduling enumeration.

## Six deliberate discrimination controls

Temporary uncommitted mutations of the gate SHA; each compiled, failed its named native assertion with exit **101**, was restored, and the same exact selected test passed with exit **0**. Controls stopped at iteration **6** (UI handle). No survivor or compiler-blocked control occurred in these six controls. The final scoped source comparison to the gate SHA exited 0.

Each command was the cargo test command above plus `TEST_NAME -- --exact`:

| Row/control | Mutation | Native assertion | Mutant exit | Restored exit |
| --- | --- | --- | --- | --- |
| chaser-class | `"ChaserOrderHandler".into() -> "WrongChaserClass".into()` | **CLASS native telemetry names** | 101 | 0 |
| twap-class | `"TwapHandler".into() -> "WrongTwapClass".into()` | **CLASS native telemetry names** | 101 | 0 |
| ui-class | `"UiStateHandler".into() -> "WrongUiClass".into()` | **CLASS native telemetry names** | 101 | 0 |
| chaser-handle | `.is_none_or(\|name\| name.is_empty()) -> .is_none_or(\|_\| false)` | **CHASER native account fallback** | 101 | 0 |
| twap-handle | `if host.manager().has_id(id) { -> if false && host.manager().has_id(id) {` | **TWAP native add before missing-state error** | 101 | 0 |
| ui-handle | `.unwrap_or(false); -> .unwrap_or(false) && false;` | **UI native symbols precede margin** | 101 | 0 |

Class controls use class_context_and_base_telemetry_are_real. Chaser handle uses chaser_routes_and_account_fallback_precede_registration. TWAP handle uses twap_restore_and_commands_pin_native_partial_effects. UI handle uses ui_symbol_error_prevents_margin_and_live_metadata_is_read_after_symbols. The three class controls assert independent class-name expectations in decoded native post payloads before loading the source oracle; they do not compare a source artifact to itself.

## Explicit boundaries and review

Base buffering/telemetry captures the constructor-time exchange; direct handle reads live context.exchange. Changing that context does not retarget existing base timers. Native replay catches/reports a synchronous handler error and continues the remaining buffered messages; source flush throws and stops. Weak ownership/Drop and canonical errors are chosen native lifecycle contracts, not JS GC/raw exception equivalence. Typed payload/schema validation omits arbitrary own/unknown/inherited properties, getters and malformed JS coercions; explicit null TWAP state is rejected before construction while source constructs/adds then catches. Missing absent state retains source add-before-catch partial effects. Additional current-record identity check suppresses stale host TWAP projections. Host must retain full lifecycle instances consistent with manager membership and release retained callback cycles on removal/teardown. New host dependencies do not claim automatic worker/bootstrap/venue wiring.

UI immediately-ready native settlement is deliberately CHOSEN: the direct native leverage is already 11 before handle returns, while actual source is 5 before its microtask and 11 afterward. Pending fulfillment follows the asserted prefix/host ownership sequence. Existing UI helper String/Store/cancellation boundaries remain independently mapped. The gate is a bounded routing/lifecycle composition proof, not complete JS scheduling or arbitrary malformed payload parity.

Read-only sub-agent context_review reviewed design, production/test code, replay/readiness additions and final mappings. No remaining blockers. Review caught missing replay/stale/readiness witnesses, overly broad note clauses and the stale pending statement; each was fixed/bucketed before final gates. The reviewer ran no gates/mutations.

Exploratory failures before the gate commits: --no-run cargo test exit 101 for an unused test import; first four-test target exit 101 for incorrect fixture key place_order instead of placeOrder. Source oracle exits 1 exposed a missing exported instance handle and then an incorrect attempt to wrap instance arrow methods through the prototype. Those fixtures were corrected, and subsequent exploratory four-test runs exited 0. An exploratory --lib cargo check exited 0; it is not presented as the all-targets gate. A test-fixture rustfmt parse failure exited 1 and was repaired before compilation.

## Not run

No workspace compilation/full monorepo suite, host clippy, wasm clippy/build, browser/SharedWorker deployment, live venue/network tests or CI rerun. Only the named command-handler target ran as the Rust behavioral gate; unchanged lifecycle/helper targets were not rerun. A scoped check does not establish workspace compilation. Review did not run tests.

## Integration merge-down and delivery

Fetched/merged integration `65e086a3c9039a79e4c7e9f354aba31d8f83009e` down into this branch at `51476d8da0e1ffbcd369199585a81cfa9ed9b746` with no conflict. `git diff --exit-code 3218911c12 HEAD -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests` measured **0**; production, tests, oracle/control sites were byte-unchanged. A separate scoped diff across all seven Rust dependencies and TS handler/TWAP/types/safe-math/logging oracle inputs also exited **0**. Cargo.lock gained a Blofin dependency and package.json changed a Kucoin command; other manifest inputs changed. Fresh all-targets check, four command-handler tests and extractor check each exited **0 at `51476d8da0e1ffbcd369199585a81cfa9ed9b746`**. Extractor text still reported 7341 symbols / 512 files / zero updates/additions/orphans. These are completed measurements, not running gates. Six controls remain measurements of temporary mutations of `3218911c12`; they were not rerun after this merge and are not represented as new-head controls. Their source, assertions and dependency implementations are byte-unchanged.

Census against merged integration still moves exactly six specified pending identities to MIXED divergent, with the same 38-file/222-row floor and 112/80/30 final worker triple. Evidence-only delivery commits do not alter gate inputs. Initial built parent remains `976c7bf08e`; additional ancestry is reviewed integration plus own reviewed slice commits, with no external unreviewed branch ancestor.

Final fetch merged integration `65e086a3c9039a79e4c7e9f354aba31d8f83009e` at `4a236ac8616a188d43ac8a1c242dbaf757a98b4c`. Source/tests comparison to `3218911c12` again exited **0**. This merge added transport dependency implementations, so the earlier seven-dependency unchanged statement applies only to the preceding `51476d8da0` measurement. Fresh all-targets cargo check, all four command-handler tests and extractor check each exited **0 at `4a236ac8616a188d43ac8a1c242dbaf757a98b4c`**; the extractor output again reported zero drift. Controls were not repeated after the transport dependency change: their measured discrimination SHA remains `3218911c12`, while restored native behavior is freshly asserted at this merge SHA. No new control coverage across changed dependency code is claimed. No CLAUDE conflict occurred on either merge. Worker census remains 112/80/30 with six moved rows.
