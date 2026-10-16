Crypto.com final pending-row slice

Completion measured at `62169a9fcbeb296c62fd6eaad34c01efb98944cd`: **119 ported /18 divergent /0 pending**, no open class FINDING; seven closures against integration ac0f5fccf2. [The completed full workspace/wasm ledger](evidence/workspace-62169a9fcb/gates.json) records all12 requested/follow-through gates at exit0, including separate host/wasm clippy and actual transport wasm execution in Node. [The extractor check](evidence/completion-62169a9fcb/extract-check.json) also exited0 with stable output. Canonical parity check/report/progress each remain1 on the inherited global indexer blocker; they are not green. Earlier sections below retain historical scopes/results and are not relabelled as new measurements.

The opening section records the historical scope through `854598a603`; later revision-named follow-ups record changed source and additional gates.

Actual branch parent: `6957adc80f485c945c18bb0d83c4965f5fe3cd95`. Integration merge parent: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. No other unmerged lane was used as an ancestor. Integration movement contributes zero Crypto.com rows to this slice.

The bounded tomllib census covers six venue manifests, stopping at index5 `cryptocom.ws-public.ts.toml`: **112 ported / 20 divergent / 5 pending → 116 ported / 21 divergent / 0 pending**. Five rows moved: `createAPI`, constructor, `start`, and `tick` became ported/adapted; the `CryptocomExchange` class became divergent/FINDING.

The class has a real native definition, prototype protocol and constructor. `RuntimeOps::existing_member` still needs concrete adapters for the older typed field bodies. Owner: cryptocom. This accepted omission prevents an assembled venue-runtime claim. Native descriptor attributes and assembled scheduling are also unproved. This is pending-row completion, with 21 divergent rows remaining.

The maintained TypeScript factory, class and full startup/polling bodies are executed by the oracle with explicit dependency spies. The native witness asserts decoded values, original identities and literal independent counts before effect comparisons. Case floor70 stops at index69 `two-fresh-instances`; own-field floor36 stops at `mapCryptocomTimeInForce`; observer-drop floor6 stops at `tick after` and proves the test provider's ownership protocol. None is an exhaustive input, schedule, host or venue certification.

Boundary declarations are maintained only in [§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary), [§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary), [§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), [§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect), [§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy), and [§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

Measured revisions and controls:

- [`44f09dcfe7`](evidence/runtime-44f09dcfe7/controls.json): seven compiling controls, stopping at index6 promise ownership. API timeout, rate limit, class name, startup delay, polling period, oracle no-op and promise ownership each produced the named runtime assertion at exit101 and a restored exit0. No survivors among these seven. Baseline: three tests passed. These are historical results at the named revision.
- [`05696fe560`](evidence/runtime-05696fe560/controls.json): three additional review controls, stopping at index2 constructor callable. Interceptor receiver, socket receiver and supplied API callable each produced a named runtime101 and restored0. Baseline: three tests passed. These results belong to this revision.
- Production source is byte-unchanged from `44f09dcfe7` through `854598a603`; source and tests are byte-unchanged from `05696fe560` through `854598a603`. The later two commits fix only runner failure propagation, log retention and portable sanitization. Original and restored mutation patches/logs are retained; compilation failures are excluded from runtime proof.

Two independent adversarial sub-agent reviews cleared the bodies, witnesses and runner fixes at `854598a603`. [Initial findings](evidence/runtime-44f09dcfe7/REVIEWS.md) and [final static receipts](evidence/runtime-854598a603/REVIEWS.md) distinguish static review from executed gates. The earlier runner verdict was DO NOT MERGE; failure propagation and overwritten-log findings were fixed before the final MERGE verdicts.

The fresh external oracle install used CI's existing `ci-deps.cjs`, the repository's unchanged frozen lockfile and Yarn1.22.21, with install scripts disabled. [Install provenance](evidence/runtime-854598a603/isolated-install-provenance.json) and the first isolated-oracles log record seven checks stopping at index6 missing compiler: runtime70 and order-writes91 source rows; timeout1→0, missing-package1→0 and missing-compiler1 with named diagnostics. No repository `node_modules` ancestor exists in the temporary source mirror. The development mirror run used the repository compiler path and is not claimed as fresh-install proof.

Final named gate exits are in the revision-pinned ledger below. Cargo commands are scoped and sequential with jobs1; host and wasm clippy are separate measurements. Yarn result text was inspected in addition to subprocess exits.

| Gate at `854598a603` | Exit | Log |
| --- | ---: | --- |
| isolated-oracles (fresh frozen install) | 0 | [output](evidence/runtime-854598a603/000-isolated-oracles.log) |
| check | 0 | [output](evidence/runtime-854598a603/001-check.log) |
| tests | 0 | [output](evidence/runtime-854598a603/002-tests.log) |
| host-clippy | 0 | [output](evidence/runtime-854598a603/003-host-clippy.log) |
| wasm-clippy | 0 | [output](evidence/runtime-854598a603/004-wasm-clippy.log) |
| wasm-build | 0 | [output](evidence/runtime-854598a603/005-wasm-build.log) |
| fmt | 0 | [output](evidence/runtime-854598a603/006-fmt.log) |
| extract-check | 0 | [output](evidence/runtime-854598a603/007-extract-check.log) |
| parity-check | 1 | [output](evidence/runtime-854598a603/008-parity-check.log) |
| parity-report | 1 | [output](evidence/runtime-854598a603/009-parity-report.log) |
| parity-progress | 1 | [output](evidence/runtime-854598a603/010-parity-progress.log) |
| ci-wiring-focused | 0 | [output](evidence/runtime-854598a603/011-ci-wiring-focused.log) |
| ci-wiring-full | 1 | [output](evidence/runtime-854598a603/012-ci-wiring-full.log) |
| isolated-oracles (repository compiler path) | 0 | [output](evidence/runtime-854598a603/013-isolated-oracles.log) |

Not run: workspace compilation/test suites, workspace host/wasm clippy, wasm execution, live venue/API/network trading, an assembled Crypto.com runtime test, exhaustive inputs/schedules, or remote CI reruns. No source TypeScript bug was changed. The original root evidence logs were preserved.

Development scheduling deviation: early CI-wiring Node invocations overlapped an active Cargo control. Review discovered that importing the Node suite invokes `cargo metadata`; this violated the one-Cargo scheduling rule. It was acknowledged, and the final gate runner serializes these commands. The integration-parent comparison was deferred until Cargo is idle; its preflight null exits are not test results.

Canonical `parity-check`, `parity-report` and `parity-progress` each exited1 before producing a validated result: `safe-cex-webworker/src/chaser_manager.rs` contains an item macro `thread_local` unsupported by the global indexer. No canonical parity-pass or validated progress claim is made. The focused Crypto.com wiring audit passed five tests; the broad wiring audit exited1. Integration-parent comparison is recorded separately. The aggregate gate runner correctly exited1 for these four failing commands. Diagnostic installation paths were redacted after capture; exits, assertion text, commands and revision identities were unchanged.

Integration baseline attribution is now measured: [parent receipts](evidence/runtime-854598a603/baseline-ac0f5fccf2/receipt.json) record the Node audit at `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`: exit1, 177 passed/86 failed; this branch: exit1, 182 passed/86 failed. The five added scoped tests pass. The independently extracted Spec-reporter failure-label sets match at a bounded floor54 unique labels (including suite labels); this is not equality proof for all assertion payloads. [Comparison](evidence/runtime-854598a603/ci-failure-label-comparison.json).

[Actual parity baseline executions](evidence/runtime-854598a603/baseline-ac0f5fccf2/parity-baseline-executions.json) ran the same three built CLIs with the workspace overridden to pinned integration input. All exited1 with the same unsupported-macro diagnostic. No Cargo/rebuild was used for these CLI baselines. Binary hashes remained stable before/after; the macro source and entire parity crate are independently byte-identical between integration and the measured branch. Canonical parity remains failed on both inputs; no gate was weakened or projected to exclude the blocker.

The delivery adds only evidence and a resolved-tool-path redaction change after the measured code revision. That redaction changes diagnostic presentation, not gate commands or exits. Rust source, tests, manifests, reference TypeScript, shared dependencies and CI wiring remain byte-identical to the measured `854598a603` revision. Exact diff receipts distinguish this claim from a new compilation.

Full workspace follow-up measured at `d821c53732e73ceaad0a0012209a415563cb8fed`:

| Gate | Exit |
| --- | ---: |
| `cargo fmt --all --check` | 0 |
| `cargo check --workspace --all-targets --locked --jobs 1` | 0 |
| `cargo clippy --workspace --all-targets --locked --jobs 1` | 0 |
| `cargo build --workspace --locked --jobs 1` | 0 |
| `node --test ../.github/scripts/rust-wasm.test.mjs` | 0 (14 tests) |

[SHA-bound receipts and full sanitized output](evidence/workspace-d821c53732/gates.json) record sequential commands from `sdk-rs`, with `RUSTFLAGS=-Dwarnings`. The prerequisite workspace check completed before host clippy. The host command accepted the whole configured workspace/all-target scope; this says nothing about wasm clippy sites.

The baseline policy wasm build was deliberately cancelled before completion to apply the owning-class bridge and measure the wasm lane against that code. Its gate exit is **unknown**, not a pass or a compiler failure. The queue process was terminated at exit143 only after all owned children were stopped; that termination is not the Cargo gate exit. [Cancellation and queued-not-run commands](evidence/workspace-d821c53732/CANCELLED.json). Wasm clippy, exchange feature builds, tooling setup, actual wasm execution and the core wasm build were not run in this baseline queue. Workspace tests, live venue/network trading and exhaustive host/input/schedule certification remain not run.

**Development Cargo-overlap deviation: I violated the one-Cargo-at-a-time rule.** The earlier CI-wiring import launched Cargo metadata while a mutation Cargo test was active. That development invocation is excluded from sequential-gate evidence. The deviation is also plainly disclosed in PR7872's body; subsequent recorded gates are sequential.

Native owning-class follow-up at `78e9ea486ec776f6849394fb7fda8cd3eb76dcad`:

[The revision-pinned scoped ledger](evidence/bridge-scope-78e9ea486e/gates.json) records exit0 for scoped all-target check, native bridge tests (21), full venue all-target tests including ignored targets, host clippy and wasm clippy. Cargo and Cargo-spawning commands ran sequentially; host and wasm are separate measurements. The full workspace host results above remain attached to d821 and are not relabelled as measurements of this changed source.

The code replaces application-member providers with concrete native class factories and ReadHost/WriteHost adapters. The native tests include successful parser callable replacement, fresh imported enum reads, iterator completion/error closing, reentrant callback factories, retained callbacks/observers, distinct REST endpoints, and fields obtained through actual exchange/public/private constructors. The source entry floor65 stops at native index64 private ping and source index64 private dispose. This entry smoke is separate from complete body fixtures and scheduling assertions. The numerical body scope remains standard parseFloat/Math Number results; the replacement-parser probes return Numbers. Arbitrary replacements returning nonnumeric values, exhaustive hosts/schedules and assembled engine certification are not claimed.

One sub-agent cleared the committed native bodies/witnesses by bounded static review. The independent runner review found a fail-open in the proposed manifest writer: a receipt count alone admitted duplicated or all-restored evidence. The repair checks the independently specified control labels, exact mutated/restored pairs, SHA, command, marker and raw runtime logs. [Synthetic Node unit discrimination](evidence/bridge-proof-review-78e9ea486e/unit-proof.json) reports a count-only guard mutation failing the named duplicate-receipts test at Node exit1 and restored exit0 (16 tests). These are explicitly synthetic admission inputs, not Rust mutation receipts. The bounded actual-source target precheck is not the canonical Rust indexer.

The three class FINDING annotations remain unchanged until all runtime mutation controls produce named failures and restored passes. The guard repair and its Rust CI wrapper will be measured at their own committed revision. Full workspace gates, policy wasm build/clippy and actual wasm execution on the completed bridge remain outstanding at this stage. No row movement is claimed for this evidence/guard step.

Scoped follow-up at `a96fe6c7952605a7b1ef8ca0a0b06f8482a7fd99`:

[Five scoped gates](evidence/bridge-scope-a96fe6c795/gates.json) each exited0: all-target check, 22 native bridge tests, full venue all-target tests including ignored targets, host clippy and wasm clippy.

[Partial runtime-control queue](evidence/bridge-controls-a96fe6c795/controls.json) stopped at control index8 `enum-comparison`. Controls0–7 each reached their named runtime failure101 and restored pass0. The ninth mutation exited101 during compilation because replacing the comparison left `value` unused under denied warnings; runtime did not execute. Its restoration passed0. This is explicitly **COMPILER-BLOCKED**, not discrimination proof, and no class row is promoted. Controls9–22 were not run in this queue. The runner restores the original bytes even on this failure, and its aggregate exit1 was measured. The repair keeps a harmless reference to `value` inside the deliberate mutation so the incorrect comparison can reach the assertion. A new committed revision will receive a fresh full control ledger; this partial ledger is preserved.

[Second partial queue at `538f24dcd549d58bb3d96554ac9f07c3709a5c07`](evidence/bridge-controls-538f24dcd5/controls.json) completed controls0–19 with named runtime101→restored0, including the repaired enum comparison. Control20 `exchange-constructor-binding` exited101 during compilation: disconnecting the only production exchange factory made eleven native administrative/exchange helper functions dead code under denied warnings. Runtime did not execute; restoration passed0, and the runner exited1. Controls21–22 were not run. The repair disconnects only the actual constructor's getAccount field and retains every other factory binding, so the integration witness can observe a wrong native field without removing the module's callers. The blocked receipt is retained, not treated as runtime proof.

[Third partial queue at `17f13f7b346218f58b0f499927b79e32d89d951b`](evidence/bridge-controls-17f13f7b34/controls.json) again completed controls0–19 with runtime101→restored0, then stopped at control20. The narrowed constructor mutation incorrectly compared the loop's `&&str` with `&str`, producing compiler E0277 instead of runtime proof. Restoration passed0 and the aggregate runner exited1. Controls21–22 were not run. The mutation repair dereferences the loop item; production source remains unchanged. A targeted preflight of controls20–22 precedes the next full ledger.

[Node-only source isolation at the same revision](evidence/bridge-isolation-17f13f7b34/receipt.json) exited0 across the independently fixed ten-check floor, stopping index9 missing compiler. The temporary source mirror had no node_modules ancestor. This invocation used the supplied repository compiler installation, not a newly installed dependency tree. It launched no Cargo process.

Owning-class closure proof at `39482ee6cebb82d53ca25a46a856027f5fe274a7`:

[The full runtime-control queue](evidence/bridge-controls-39482ee6ce/controls.json) exited0 with the independently specified23 controls/46receipts, stopping index22 `source-noop`: every deliberate mutation reached its named runtime panic/assertion at101, and every restored test passed0. No survivors or compiler-blocked controls occurred in this complete queue. The first series control uses the standard RefCell borrow panic in its named test. The await-turn control is a witness-model control, not real-engine scheduling proof. The [separate last-three preflight](evidence/bridge-controls-preflight-39482ee6ce/receipt.json) is scoped3, not23; its transformation wrapper is retained. All mutation bytes are restored.

[Guarded manifest reconciliation and writer receipts](evidence/bridge-reconcile-39482ee6ce/receipt.json) validate those actual raw logs before promoting the three class roots. All137 symbol identities and prior Rust targets are retained. Status changes are exactly exchange/public/private class roots, divergent→ported; tomllib after reconciliation is119ported/18divergent/0pending across six files, stopping index5 cryptocom.ws-public.ts.toml, with no remaining FINDING prefix. Against integration ac0f5fccf2, the total closure is seven rows: five pending→ported plus the two original socket class FINDING→ported. The exchange's provisional FINDING is not counted twice. The extractor writer exited0 and its output reports0updated manifests,0added and0orphaned symbols. These writer receipts identify the precommit result by manifest hashes; they do not claim the base394commit already contained the promoted statuses.

Production Rust, tests, maintained Crypto.com TypeScript and workspace Cargo inputs are byte-identical between scoped-gate SHA a96fe6c795 and control SHA39482ee6ce (recorded git diff exit0). Scoped check/tests/host clippy/wasm clippy results remain attached to a96, not relabelled. Full workspace fmt/check/clippy/build and the complete policy wasm lane including actual execution on the completed bridge are next; no final exit is claimed before completion. The source entry floor65 remains smoke, separate from body/value/identity/ownership tests, and no assembled engine, live venue or exhaustive host/input/schedule certification is claimed.

[Sub-agent findings and bounded follow-up](evidence/bridge-reconcile-39482ee6ce/REVIEWS.md) distinguish static review from executed controls. The development Cargo-overlap deviation remains disclosed above and directly on PR7872.

Completed full workspace and configured wasm lane at `62169a9fcbeb296c62fd6eaad34c01efb98944cd`:

| Named gate | Exit | Log |
| --- | ---: | --- |
| `fmt-all` | 0 | [output](evidence/workspace-62169a9fcb/00-fmt-all.log) |
| `workspace-check-prerequisite` | 0 | [output](evidence/workspace-62169a9fcb/01-workspace-check-prerequisite.log) |
| `host-clippy-workspace` | 0 | [output](evidence/workspace-62169a9fcb/02-host-clippy-workspace.log) |
| `build-workspace` | 0 | [output](evidence/workspace-62169a9fcb/03-build-workspace.log) |
| `wasm-policy-audit` | 0 | [output](evidence/workspace-62169a9fcb/04-wasm-policy-audit.log) |
| `wasm-policy-build` | 0 | [output](evidence/workspace-62169a9fcb/05-wasm-policy-build.log) |
| `wasm-policy-clippy` | 0 | [output](evidence/workspace-62169a9fcb/06-wasm-policy-clippy.log) |
| `wasm-exchange-default` | 0 | [output](evidence/workspace-62169a9fcb/07-wasm-exchange-default.log) |
| `wasm-exchange-all-features` | 0 | [output](evidence/workspace-62169a9fcb/08-wasm-exchange-all-features.log) |
| `wasm-ffi-setup` | 0 | [output](evidence/workspace-62169a9fcb/09-wasm-ffi-setup.log) |
| `wasm-transport-execution` | 0 | [output](evidence/workspace-62169a9fcb/10-wasm-transport-execution.log) |
| `wasm-core-build` | 0 | [output](evidence/workspace-62169a9fcb/11-wasm-core-build.log) |

[Exact commands, SHA, environments and elapsed times](evidence/workspace-62169a9fcb/gates.json) and [completed queue/scope receipt](evidence/workspace-62169a9fcb/COMPLETION.json). The aggregate queue exited0; floor12 stops index11 wasm-core-build. Cargo and Cargo-spawning commands ran sequentially. The workspace check accepted all targets before host clippy; the full policy wasm build passed before independent wasm clippy. Both policy helpers selected the configured38-package floor, stopping policy index37 safe-cex-webworker, including the FFI wasm feature. This is not an all-possible-features claim. The separate exchange default/all-features builds also passed.

The execution command built the Rust transport wasm example, processed it with the locked wasm-bindgen CLI, loaded it in Node and asserted warning output/deduplication, real JS reentry, direct routes, relay overlay and tunnel endpoints. Its exit0 and verification output are retained. This is actual wasm execution, not compilation, and is not an assembled Crypto.com engine certification.

[Independent tomllib census](evidence/completion-62169a9fcb/census.json) retains the six-file floor ending index5 cryptocom.ws-public.ts.toml, all137 symbols and exact seven status changes against pinned integration. [Scoped-gate input identity](evidence/completion-62169a9fcb/source-identity.json) is git diff0 from a96fe6c795 through621; the scoped venue tests remain measured at a96. [Extractor check](evidence/completion-62169a9fcb/extract-check.json) exited0; output explicitly says Would update0 manifests,0added,0orphaned.

[Current canonical CLI receipts](evidence/completion-62169a9fcb/canonical-parity.json): parity-check1, parity-report1, parity-progress1, each blocked before validation by safe-cex-webworker/src/chaser_manager.rs item macro thread_local. Direct built binaries launched no Cargo subprocess. The collection helper's exit0 means collection completed, not that a CLI gate passed. [Tool/macro source identity](evidence/completion-62169a9fcb/canonical-input-identity.json) is git diff0 against ac0f5fccf2; historical pinned-parent executions remain separately recorded above. No canonical validated progress/parity claim is made.

Two final bounded sub-agent MERGE reviews at621 are retained: [native bodies/witnesses](evidence/completion-62169a9fcb/RUNTIME_REVIEW.md) and [proof admission/manifest preservation](evidence/completion-62169a9fcb/PROOF_REVIEW.md). They distinguish read-only audits from the executed Cargo gates and preserve the initial failed broader-note audit. Their verdicts cover reviewed code/evidence; the full gate queue completed separately afterward.

Not run: workspace test suites, full FFI browser/feature execution matrix, live venue/API/network trading, assembled Crypto.com engine certification, exhaustive inputs/hosts/schedules or remote CI reruns. Broad CI-wiring was not rerun at621; its earlier paired branch/integration failures remain historical measurements. The earlier **development Cargo-overlap deviation** remains plainly disclosed in this file and PR7872 itself. Actual fresh parent6957adc80f485c945c18bb0d83c4965f5fe3cd95; merged integration parentac0f5fccf24207b26c279e67da2a88f2c72a7aae. No unrelated unmerged lane ancestor.

## Production binding: preserve REST currency output (2026-10-16)

Integrated the shared CurrencyBalance implementation from #165 (61d2418fe7,
merged integration 3b2483c4bb). Removed project_balance and its Store wrapper;
validation now uses the same typed conversion as every other trait read.
Source fetchBalance's USD fixture (quantity15, max_withdrawal_balance9) reaches
both ValidateAccountResult and Store as AvailableLocked{available9,locked6},
with exactly those two serialized field names. No free/used/total/upnl fields
are invented for this source variant.

The complete scoped venue suite passed 102 tests (27 targets including empty
doctests), zero ignored, with actual TypeScript oracles enabled. All 22 compiling
binding controls failed their named runtime test and passed after exact byte
restoration: 19 method/disposal controls and 3 signing/retry globals controls.
The validate_account control now redirects the production trait invocation;
it no longer targets the removed normalization helper. Exact enum and JSON
field assertions are in tests/exchange_binding.rs.

Strict scoped host all-targets and wasm library Clippy passed with -D warnings.
Canonical workspace parity passed: 7459 rows / 17049 public Rust items. Crypto.com
has no FINDING rows after removing the currency normalization. Extractor check
has no Crypto.com drift; remaining base.ts/safeCexAxiosCreate metadata formatting
belongs to the core lane. Currency consumer correction caa2395524 was merged
before pushing; it changes one Binance test and canonical TOML spacing only.
