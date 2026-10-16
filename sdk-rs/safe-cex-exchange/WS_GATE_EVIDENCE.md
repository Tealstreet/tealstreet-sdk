# Websocket gate evidence

The final crate-wide gates are recorded by the implementer in `GATE_EVIDENCE.md`. This file records the socket-specific checks and controls. All credentials and hosts in tests are synthetic.

| Check | Actual outcome | Negative control |
| --- | --- | --- |
| Socket unit tests, including the live TypeScript oracle | 26 passed | Disabled `send`'s disposal guard: the intended sends-after-dispose assertion failed. Restored source passed. |
| Reopen coalescing | Passed, including sibling sockets with colliding host labels | Removed the pending rerun flag: the assertion requiring exactly one follow-up snapshot failed. Restored source passed. |
| Diagnostic rate limit | Passed | Changed the cap from 200 to 201: the intended emitted-frame count failed. Restored source passed. |
| Live TypeScript source oracle | Passed: 130 heartbeat/close/redaction vectors, plus live stats and backoff sequences | Mutated the real TypeScript literal-heartbeat branch in memory: Rust/TS comparison failed specifically on `ping`. Restored source passed. |
| Default wasm target build | Passed after the final relay-overlay and retained-task disposal fixes | Added `std::os::unix::net::UnixStream`: the actual wasm build rejected it with the missing `unix` diagnostic. Restored source passed. |
| Real parent bridge / exchange lifecycle tests | 8 passed | Tests exercise socket replacement clearing optional evidence, duplicate labels retaining object identity, deferred auth acknowledgement, weak venue drop, and per-open watchdog grace. |
| Strict clippy | `cargo clippy --locked -p safe-cex-exchange --all-targets --all-features` passed | Crate-wide lint mutation is recorded in the aggregate gate evidence. |
| Owned source formatting | `rustfmt --check --edition 2024` passed for the owned Rust files | Crate-wide formatting mutation is recorded in the aggregate gate evidence. |
| Health feature policy | Five Node tests passed against actual Cargo declarations and resolved dependency trees | Metadata clones removing facade health, adding core default health, and making core regex mandatory each failed for its intended named policy reason. |

The live oracle executes the actual TypeScript helpers/class using the installed compiler and deterministic host boundaries. It does not read a frozen expected-output file. Run it with `cargo test --locked -p safe-cex-exchange --lib live_ws_typescript_oracle -- --include-ignored`. CI must install TypeScript and run `--include-ignored` in the same change that introduces the ignored test.

`node safe-cex-exchange/scripts/ws-negative-controls.mjs` runs the four source/test controls and the wasm control, requires the intended failure diagnostic, restores the file in `finally`, and verifies the restored gates. Run from `sdk-rs`; Cargo must be on `PATH`. The TypeScript mutation changes an in-memory source copy and never modifies the TypeScript repository file.

`node --test safe-cex-exchange/scripts/health-features.test.mjs` audits the actual optional-health policy from `sdk-rs`. It runs locked metadata/tree commands and does not pin a regex version or read cached graph fixtures.

An adversarial detached review reproduced a panic when a host-managed reconnect synchronously disposed the socket while the core retained a state borrow. The retained regression now passes: native handles are cloned into locals before host calls; task-handle destruction also occurs after releasing state borrows. The relay integration tests exercise the real transport owner: static header propagation, sealed Origin/UA/ALPN policy, protocol forwarding, lazy fresh providers across two inner dials, and required-header refusal before provider evaluation or dialing.

Retained-task disposal follow-up: the actual pending `on_open` path releases its socket after disposal and scheduler cancellation, a never-woken native receive is cancelled, and inline completion/disposal leaves no registered handle. Negative control removed cancellation of owned tasks (retaining timer cancellation): both regressions failed specifically with `disposed auth/event task retained socket` and `pending native receive survived disposal`. Disabling the inline-completion guard failed the empty-owned-task assertion. All mutations were restored before the final checks.

### Lazy-host interval registration

The original owned-timer mock recorded rearming when `sleep()` constructed a
future. That proves construction order, not registration order for a host that
registers its timer on the future's first poll. The strengthened mock records
registration on first poll and tracks release separately. Core primes the next
sleep before reporting the callback error and explicitly yields before the next
callback, retaining a completed sleep outcome without repolling its future.

The focused tests cover lazy registration, reentrant reporter cancellation,
ready-success yielding, callback-error before ready-sleep-error, and cancellation
during the priming poll. Removing priming compiled and exited 101 at
`INTERVAL-ORDER rearm precedes reentrant reporter`. Removing the explicit yield
compiled and exited 101 at
`INTERVAL-READY next callback requires a scheduler turn`.
These are injected regression controls, not observations of production-host
implementations. The earlier published constructor-observing evidence does not
establish lazy-host registration; consumer runtime migrations remain separate.
