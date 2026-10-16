# GRVT private websocket command gates

Implementation SHA: `3e2e60d537d64a458b1cd0bbc951aa55e7981ace`.
Actual starting integration parent: `58a3af54453ee8042113e65be02f3f0904cf1ee2`.
Manifest correction SHA: `aabb8dd842bb6c79053ef299fcbf227dcbf72743`.
Fetched integration SHA: `976c7bf08eb0d23329d7dfd9042035e3aabe2358`.
Actual merge-down SHA: `7c8b2ddbc36bfc262008eb1544499cde4c5e5e19`.
Branch: `feat/rust-grvt-private-commands`. No unmerged feature branch was used as an ancestor; both base and merge-down parent are integration commits.

## Rows moved and scope

Moved **9 pending → ported, 0 → divergent**: ping, getWatchdogStaleThresholdMs, onOpen, authenticate, resetRetryBudgetAfterHealthySession, failClosedForAuthentication, subscribe, subscribeToStream, dispose.

Independent `/opt/homebrew/bin/python3` tomllib census reads git objects across the nine GRVT manifests. Fixed expectations: 164 rows, exactly nine pending→ported transitions. At starting parent versus implementation, and again at fetched integration versus merge-down, 56 ported / 47 divergent / 61 pending becomes **65 / 47 / 52**. Census enumeration stops at those nine GRVT manifests; behavior proof is separately bounded below.

Native command functions bind the original clock/logger/send/dispatch/timer/base-disposal operations. All three private streams share one live feed Rc, also retained by stored legacy subscriptions. Authentication retains its native optional account slot through JS stringify; default logger severity stays unspecified. Payload capture, state changes, healthy-marker rereads after the clock, map insertion before send, cancellation truthiness and early error termination follow source ordering. Full private class/constructor, watchdog, asynchronous refresh, message/update handlers and fetch throttles remain pending.

## Gates at the implementation SHA

All cargo invocations were sequential, jobs 2. Node versions measured: 24.20.0 and 26.7.0. Compiler path for every native/source test: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Command | Measured exit / observation |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0; compilation finished |
| `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --test private_commands --jobs 2` | 0; nine tests passed, Node 24 |
| Same test command with `/opt/homebrew/opt/node@26/bin` prepended to PATH | 0; nine tests passed, Node 26 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; output: `Would update 0 manifests; 0 added, 0 orphaned symbols.` |

The first extractor check validated shape at the implementation SHA, not Rust target resolution. Sub-agent review found incorrect `sdk-rs/` prefixes and missing exact-target rename tokens. Manifest-only correction aabb8dd842 removes the prefixes and names each actual Rust target. Writer ran after each manifest edit through the pinned Yarn JS entry, each exit 0 and output `Updated 1 manifests; 0 added, 0 orphaned symbols.` Manifest arrays were serialized/extracted, never hand-spelled.

The oracle extracts the nine actual TypeScript members, actual timing constants and actual imported LogSeverity enum. Native request fields, numerical values, optional slots, error name/message, state, map contents, feed Rc identity and operation order are asserted before deferred wire strings. Expected counts are fixed independently. Non-NaN Number bits are compared; NaN classification is compared without claiming Node/CPU-dependent NaN payload equality.

The fixed floor stops at 67 cases: threshold 1, ping 8, open 7, authentication 9, retry budget 9, fail-closed 9, subscriptions 10, stream registration 8 and disposal 6. Witnesses include clock/log/send/emission/cancellation/base-disposal failures; callback-driven state/callee replacement; live feed mutation; signed zero, NaN, Infinity and 2^53 IDs; zero/absent timers; absent/empty account and cookie; and the inclusive healthy-session boundary. No exhaustive domain claim.

## Deliberate compiling controls

Each control temporarily changes only private_commands.rs against the implementation SHA and runs the Node 24 scoped test command above plus `<target> -- --exact --nocapture`. Every broken run compiled, entered the named test and panicked at the named native assertion; none is a compiler failure. Source bytes restored before every passing run. Final source/tests diff against the implementation exited 0. No survivors.

| Source method / mutation | Exact target | Named failing assertion | Broken exit | Restored exit |
| --- | --- | --- | --- | --- |
| threshold 90000 → 90001 | threshold_native | THRESHOLD native value | 101 | 0 |
| ping clock assignment + 1 | ping_native | PING native clock | 101 | 0 |
| onOpen sets authenticated true | open_native | PRIVATE OPEN native reset before log | 101 | 0 |
| authentication cookie rm=true → rm=false | authentication_native | AUTH native cookie | 101 | 0 |
| healthy reset threshold 300000 → 300001 | budget_native | BUDGET native attempts | 101 | 0 |
| failClosed authenticationFailed false | closed_native | CLOSED native failed before emit | 101 | 0 |
| subscribe first stream v1.WRONG | subscriptions_native | SUBSCRIBE native stream | 101 | 0 |
| subscribeToStream is_full true | stream_native | STREAM native lite flag | 101 | 0 |
| dispose retains processed fill IDs | dispose_native | DISPOSE native fills clear before timer | 101 | 0 |

Sub-agent review_snapshots approved immutable corrected SHA aabb8dd842bb6c79053ef299fcbf227dcbf72743, confirming nine mappings and the 67-case floor. Reviewer ran no cargo/gates or mutation proof; gate exits above are measured by the parent.

## Merge-down measurements

Immediately before delivery, fetched and merged integration down to the merge SHA named above. No conflicts. `git diff --exit-code 3e2e60d537d64a458b1cd0bbc951aa55e7981ace HEAD -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests packages/safe-cex/src/exchanges/grvt/grvt.ws-private.ts packages/logging/src/LogSeverity.ts` exited **0**. Source, tests and actual oracle inputs are byte-identical. Behavioral tests and controls remain measurements at the implementation SHA; they were not rerun at the merge SHA.

Cargo.lock changed during merge-down. At merge SHA 7c8b2ddbc36bfc262008eb1544499cde4c5e5e19, the exact scoped cargo check above reran: **exit 0**, compilation finished. Pinned extractor check reran: **exit 0**, output `Would update 0 manifests; 0 added, 0 orphaned symbols.` This check includes the corrected manifest. `git diff --check` exited 0. The following evidence-only commit changes this manually authored document; no runtime gate is claimed measured at that evidence-only SHA.

Not run: workspace compilation; full monorepo or full crate suites; host clippy; wasm compilation/clippy/tests; Rust parity-validator execution; live network/private socket/authentication integration; pending private methods; exhaustive enumeration; custom/proxy object or mutated Object.prototype execution; engine-specific NaN payload proof. Existing public tests were not rerun because their production implementation and oracle inputs were unchanged. No Rust parity-validator green claim.
