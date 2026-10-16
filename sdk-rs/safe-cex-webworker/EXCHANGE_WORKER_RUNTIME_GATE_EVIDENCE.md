# ExchangeWorker runtime slice gate evidence

Measured implementation/test/manifest SHA: `b7d99e0bfbc9c8d79aa6af78e7efc4f57197fe37`. Actual fresh branch parent:
`41505c870788695cb1e3201843360997f3d22794` (integration); no stranded or unreviewed feature-branch ancestor.
Branch: `feat/rust-webworker-exchange-runtime`. Evidence is a subsequent tracked
commit; gates below bind to the measured SHA, not silently to the evidence commit.

## Rows

Independent Python 3.12 tomllib census of 38 tracked WEBWORKER manifests, matched
by (ts,kind) against the parent: **10 moved: 2 pending -> ported, 8 -> divergent**.
Before 113 ported / 87 divergent / 22 pending; after 115 / 95 / 12, same 222 rows.
Ported: store/exchange getters, preserving opaque identity. Divergent: class,
constructor, throttle/cancellation fields, store_update, snapshot, dispose, clear
pipeline. Each row has its own canonical CHOSEN or MIXED reason.
Enumeration stops at these 38 files/222 rows; this is a census, not full parity proof.
Four startup/routing/initialization/event bodies, five heavy chaser bodies, and
three store-worker lifecycle rows remain pending. BREAKOUTPROP has not started.

## Measured gates

Commands run in this worktree, one cargo at a time. Cargo path:
`/opt/homebrew/opt/rustup/bin/cargo`. Tests use
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --all-targets --jobs 2`: exit 0.
- `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker --test exchange_worker_runtime --test store_preparation --jobs 2`: exit 0; runtime 4 passed, preparation 1 passed.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit 0, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` followed by `Done in`.
- Manifest modified using parseManifest/serializeManifest, then the actual writer
  `parity:extract` ran: exit 0, output `Updated 0 manifests; 0 added, 0 orphaned symbols.`
  Writer executed before the measured commit; the fresh check above measures that commit.

## One discriminating mutation per family

Temporary production changes were made only against the measured SHA. Every
control compiled and failed its named native assertion (exit 101), then exact
restoration passed its same test (exit 0, 1 passed/3 filtered). Restored scoped
`git diff --exit-code b7d99e0bfbc9c8d79aa6af78e7efc4f57197fe37 -- sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests`: exit 0.
No survivors or inconclusive controls.

Each command is `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-webworker
--test exchange_worker_runtime <test-name> --jobs 2 -- --exact` with the env above.

| Family and deliberate change | Test / named assertion | Mutated / restored exits |
|---|---|---|
| construction: Constructor multiplier base 250 -> 500 | `construction_retains_handles_and_binary64_throttle_configuration` / `WORKER native default wait` | 101 / 0 |
| throttle: Disable invoking branch (`if invoking && false`) | `throttle_leading_trailing_maxwait_rollback_and_cancel_zero` / `WORKER native leading delivery` | 101 / 0 |
| pipeline: Skip dependent processor index zero | `store_pipeline_live_arrays_failure_cutovers_and_forced_snapshot` / `WORKER native dependent before independent` | 101 / 0 |
| teardown: Suppress token increment before cancellation | `dispose_token_cancel_exchange_and_foreach_retained_array` / `WORKER native token before cancel` | 101 / 0 |

## Proof floor and limits

Native assertions execute before each source comparison. They inspect actual Rc
identity, retained input handles, live list replacement, enum order, typed payload
fields, nonfinite f64 bits, cancellation identity and live disposed/stopped flags;
no decode/re-encode comparison. Timer presence is asserted inside the leading
callback, not only after call returns. Independent fixed expectations include
nine constructor tags, seven waits, six deliveries through time 1050, and the two
reentrant/cancel deliveries through time 101. Pipeline and teardown stop at the
scenarios shown in the tests; no claim covers every clock, object or failure cut.

The oracle executes the actual TS ExchangeWorker, DefaultStore, VirtualClock
class and installed lodash throttle; nine uncalled handler constructor dependencies
are fixtures. The actual source ChaserOrderManager executes its empty-list path
in paired teardown probes. Real native manager-warning/venue-error probes are
additional native assertions, not source-paired error proofs. Heavy manager
supervision/cleanup/completion fixture methods panic if unexpectedly invoked;
production dependencies are mandatory, without default-success substitutes.

Read-only sub-agent review cleared the restored measured commit. It found and
we fixed original-input identity and inside-callback timer-order proof gaps;
matching reentrant call/cancel traces were added before final gates. Reviewer ran
no gates or mutations.

Chosen limits include opaque handler tags rather than constructor functions,
ordinary cancellation receiver binding, omitted lodash flush/function properties,
Weak/Drop lifetimes, typed Result errors and dense arrays. Mixed rows also name
shared UTF-8 String ingress and existing nested snapshot boundaries. Startup,
registry factories and event installation remain individually pending.

Exploratory uncommitted checks failed for a nonexistent cooldown-map alias,
unused fixture code/results, and JSON integer-vs-f64 source comparison; fixed.
Pre-strengthening 6ea7053300 checks/tests/extractor passed but are not final gate
coverage. An overbroad local log predicate rejected the literal `console.error`
in yarn's echoed command despite the green extractor footer; fixed to inspect
that footer before any mutation ran. No failed exploratory run is claimed green.

Not run: workspace build/check, full crate/repository test suites, host clippy,
wasm check/build/clippy, browser/live-exchange tests or CI. No shared type/trait
was changed by this slice, so the shared-change workspace sweep rule is not triggered.

## Fresh gates after integration merge-down

Merged integration `b5ac6229e2502e48106b0a549da0524583b04627` into this branch,
producing measured merge SHA `b4dfa512d5adffbd301f37b7f13ade9cb9797296`.
No conflict. Scoped src/tests diff against reviewed `b7d99e0bfb`: exit 0;
WEBWORKER manifests also byte-unchanged. Exchange dependency source and package.json
extractor inputs changed, so all measured gates and four family controls were
actually refreshed against this merge SHA, not transferred silently.

- `check`: exit 0 (89.45 seconds); same exact command above.
- `tests`: exit 0 (68.78 seconds); same exact command above.
- `extract-check`: exit 0 (36.8 seconds); same exact command above.
Tests: 4 runtime + 1 preparation passed; extractor output again `Would update 0 manifests; 0 added, 0 orphaned symbols.` and `Done in`.

- `construction`: `WORKER native default wait` failed exit 101, exact restoration passed exit 0 (1 passed/3 filtered).
- `throttle`: `WORKER native leading delivery` failed exit 101, exact restoration passed exit 0 (1 passed/3 filtered).
- `pipeline`: `WORKER native dependent before independent` failed exit 101, exact restoration passed exit 0 (1 passed/3 filtered).
- `teardown`: `WORKER native token before cancel` failed exit 101, exact restoration passed exit 0 (1 passed/3 filtered).
No survivors/inconclusive controls. Own source/tests restored byte-identical to the measured merge SHA; scoped diff exit 0. No additional test/build/CI scopes were run.

## Final merge-down preservation

Fetched and merged integration `983e9b13f32231bad4eb5c2cf76b6cd0c2a8adde`,
producing `1c76225873c4ea95b58c736fc8ada382016ab0df`, without conflicts.
Explicit subprocess-measured scoped `git diff --exit-code b4dfa512d5 HEAD --
sdk-rs/safe-cex-webworker/src sdk-rs/safe-cex-webworker/tests`: exit 0.
Compiled native source/Cargo/build inputs and source-oracle inputs are unchanged
from the fresh gate SHA; its scoped check, five tests and four mutation/restoration
exits still stand. No claim of fresh cargo measurement at this later merge.
Extractor inputs changed in other lanes: actual fresh `parity:extract:check` at
`1c76225873c4ea95b58c736fc8ada382016ab0df`: measured exit 0, green output
`Would update 0 manifests; 0 added, 0 orphaned symbols.` / `Done in 31.14s`.
Independent tomllib recount still 115 ported / 95 divergent / 12 pending,
with the same 2 pending->ported and 8->divergent moves against actual parent.
This following evidence-only commit does not change those measured inputs.
