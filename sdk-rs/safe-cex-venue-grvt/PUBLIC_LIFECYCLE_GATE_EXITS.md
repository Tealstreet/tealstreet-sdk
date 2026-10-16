# GRVT public websocket lifecycle

Implementation SHA: `dd64356a6e3fda91a527ee0a47cba20d344acd7f`.
Actual initial parent: `d4d9ae0b019e4c5466a67294aa474f897ee38061`.
Fresh branch: `feat/rust-grvt-public-lifecycle`.

Six rows move pending→ported: `connectAndSubscribe`, `onOpen`, `onMarketsChanged`, `resubscribe`, `subscribeToAllTickers`, `dispose`. Zero new divergent rows. Independent Python tomllib census of git objects across the nine tracked GRVT manifests measures 164 rows at parent and implementation: 50/47/67 ported/divergent/pending becomes 56/47/61. Enumeration stops at those nine GRVT manifests; no project-wide completeness claim.

Lifecycle functions execute against configured native factory/listener/emitter/timer operations. Socket and handler slots are read at source call sites, including replacement during earlier operations. Market events and captured batch slices hold references to the shared Market type; the minimal declared two-field event shape has its own AddedMarket projection through the same MarketIdentity reads. No shared Market fields or sidecar were added. Live event arrays, scheduling array length, retained row references and error-time symbol reads preserve the measured mutation cases. SubscriptionMap holds insertion slots through nested replay, deletion/reinsertion and clear/add, compacting unused slots after the outermost iterator ends.

## Gates measured at the implementation SHA

All cargo invocations ran sequentially with jobs 2.

| Command | Measured exit and observed outcome |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0; scoped compilation finished |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test public_lifecycle --jobs 2` | 0; seven tests passed under Node 24.20.0 |
| Same lifecycle test command with `/opt/homebrew/opt/node@26/bin` prepended to PATH | 0; seven tests passed under Node 26.7.0 |
| Same scoped test command with `--test public_subscriptions`, Node 24 | 0; nine previously ported subscription tests passed against the changed backing map |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; OUTPUT: `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

Extractor writer ran after the manifest edits through the pinned Yarn JS entry: exit 0; OUTPUT said one manifest updated, no added/orphaned symbols. Manifest arrays were emitted by TOML serializer/extractor rather than hand-spelled.

The oracle extracts the actual six lifecycle methods and their already-ported cancellation/ensure/ticker dependencies, plus the actual WSS URLs. The floor stops at 9 connect cases, 9 open cases, 11 market-event cases, 9 replay cases, 16 ticker-batch cases, 5 disposal cases and one minimal-event witness: 60 fixed cases. Native socket URL/receiver/handler identity, emitter arguments, RPC fields, map keys/state, generation, timer handles/delays, errors and operation order are asserted before deferred wire comparisons. Counts have independently fixed expectations. Non-NaN f64 bits and NaN classification are asserted; engine/CPU-dependent NaN payload bits are not claimed equal.

The 41-market case independently expects three batches and delays 0/100/200ms. Fixed witnesses cover stale/disposed/disconnected/NaN generation callbacks; inline timer ReferenceError; schedule-time array append; live ID mutation; row replacement retaining the captured old row; event append/removal and error-time symbol mutation; handler replacement between registration/off/on/clear operations; nested replay followed by clear/add. These witnesses do not imply exhaustive enumeration.

## Deliberate compiling controls

Controls ran against the implementation SHA before merge-down, modifying only `src/public_lifecycle.rs` temporarily. Each uses the scoped lifecycle test command plus `<test_name> -- --exact`. Every failure compiled and entered the named test, then panicked at the listed native assertion. Original bytes were restored before each passing target. No controls survived; no compilation-only failure is counted.

| Row / mutation | Exact test | Named failing assertion | Broken exit | Restored exit |
| --- | --- | --- | --- | --- |
| connectAndSubscribe: invert testnet environment selection | `connect_native` | `CONNECT native URL` | 101 | 0 |
| onOpen: wrong emitter event name | `open_native` | `OPEN native emitter event` | 101 | 0 |
| onMarketsChanged: wrong instrument selector | `markets_changed_native` | `MARKETS native selector` | 101 | 0 |
| resubscribe: wrong replay stream | `replay_native` | `REPLAY native stream` | 101 | 0 |
| subscribeToAllTickers: batch size 21 instead of 20 | `ticker_batches_native` | `BATCH BEFORE native timer count` | 101 | 0 |
| dispose: retain ticker entries instead of clearing | `dispose_native` | `DISPOSE native ticker cleared before emitter` | 101 | 0 |

Previously ported mutation matrices were not repeated. Their subscription test file was rerun because its map implementation changed.

Sub-agent `review_snapshots` approved immutable implementation SHA `dd64356a6e3fda91a527ee0a47cba20d344acd7f` and its stated parent, including all six ported mappings and the minimal event projection. Review caught early-captured handler references during development; live accessors and replacement witnesses fixed them before this SHA. Review is code approval, not a gate exit: the reviewer did not run cargo/source gates or certify the mutation controls.

## Merge-down measurements

Fetched `origin/feat/safe-cex-rust` and merged integration SHA `852b13d6c0385f0f1ae607af880953c335f61726` down. Actual merge SHA: `9786d0d5a16929252f7573c2672a8be4118e6171`.

`git diff --exit-code dd64356a6e3fda91a527ee0a47cba20d344acd7f HEAD -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests packages/safe-cex/src/exchanges/grvt/grvt.ws-public.ts packages/safe-cex/src/exchanges/grvt/grvt.types.ts` exited 0 at the merge SHA. Venue code/tests and actual oracle inputs are byte-identical. Behavioral/control exits above remain implementation-SHA measurements; they were not rerun at the merge SHA.

Shared dependency files changed, so the exact scoped cargo-check command was rerun at the merge SHA: exit 0, compilation finished. Pinned extractor check reran at the merge SHA: exit 0; OUTPUT says zero manifest updates/additions/orphans. `git diff --check` exited 0. Independent census at fetched integration and merge again measures 164 GRVT rows: 67 pending before, 61 after, the same six pending→ported movements.

The branch starts from the supplied integration head, with no unmerged feature branch used as its base. Merge-down parent is fetched integration. The following evidence commit changes only this manually written document; no gate is claimed at that evidence-only SHA.

Not run: workspace compilation, full suites, host clippy, wasm check/clippy/tests, live sockets/network requests, old subscription tests under Node 26 at this implementation SHA, the full public class/constructor, pending message/feed/listener functions, exhaustive input enumeration, custom/proxy/mutated Object.prototype execution, or engine-specific NaN payload proof. Inherited Rust parity-validator diagnostics were not rerun or claimed green. This evidence covers the named scoped checks, extractor, native/source tests and compiling controls.
