# Eager dial fixture sweep

Reviewed from mirror master `052623e563` after shared eager-start #1130 and
CONNECTING-handle #1150. This sweep changes fixtures only.

## Maintained source evidence

`packages/safe-cex/src/exchanges/bybit/bybit.ws-public.ts::connectAndSubscribe`
and `bybit.ws-private.ts::connectAndSubscribe` synchronously assign
`this.ws = this.createWebsocket(...)` before adding their listeners and returning.
`packages/safe-cex/src/exchanges/base.ws.ts::createWebsocket` constructs the
WebSocket/ReconnectingWebSocket, registers its error/open listeners, then returns
that handle. A held handshake does not defer this prefix until a Rust task poll.
This does not assert that a network upgrade or Promise continuation is synchronous.

The existing maintained-member oracle `ws-dial-fixtures.mjs` still executes all
720 original cases, six connections each, with exact URLs, headers, branches and
emitter effects. It now independently asserts the cumulative socket-create count
immediately after **each** call: one per admitted connection, zero for disposed
or private payload-signer admission. No await or timer runs between call and check.
The native comparison retains both deferred and inline schedulers (1,440 cases).

## Search scope and disposition

Searched **all 27** `safe-cex-venue-*` crate trees, including nested `src` test
modules, integration tests, support files and source oracle scripts. The search
covered `unpolled`, `lazy` dial/connect, `before` poll/pump, inline-dependent
counts, and zero/empty dial, URL, connection, socket and trace assertions.
Multiline assertion blocks were also inspected, rather than only single-line
`assert!` spellings. This is a fixture census, not a claim that every venue's
unrelated suite was executed.

Representative reproducible searches, from the repository root:

```sh
rg -n -i 'unpolled|lazy.{0,20}(dial|connect)|usize::from\(inline\)|before.{0,25}(dial|poll|pump)' sdk-rs/safe-cex-venue-* -g '*.rs' -g '*.cjs' -g '*.mjs'
rg -n 'dials|dial_options|urls\.borrow|sockets\.borrow|connections|trace' sdk-rs/safe-cex-venue-* -g '*.rs'
```

| Venue crates | Reviewed disposition |
| --- | --- |
| Bybit | Stale `dial_tests.rs` expected zero direct dials before polling and immediate removal of the host scheduler task. Updated below. Constructor-no-dial and reconnect/auth gating checks remain. |
| Binance | `public_connect.rs` already repaired by merged #1150: immediate dial 1 for both inline modes, pending I/O drop before pump, driver reaping after pump. No duplicate edit. Private-class zero counts wait for listen-key Promise reactions; inverse transport does not connect; failed listen-key retry has no socket. These guards remain. |
| Backpack, Bitget | Existing eager-prefix fixtures already expect immediate dial 1. Dropped-parent refusal remains zero. Fake WsRuntime pending-handle assertions describe that fake host; real native CONNECTING presence is separately covered by #1150. |
| OKX | Skipped-heartbeat log failure must not reconnect. Zero dial remains valid. |
| WEEX, Hyperliquid | Constructor-only cases intentionally create no connection. External startup guards remain. |
| Orderly, AsterDEX | Failed authentication/admission prevents socket startup. Zero dial remains valid. |
| Coinbase, Crypto.com, Polymarket, Phemex | Inert startup/capability, failed Store notification, disposal during held HTTP, and no-extra-recovery-chain guards remain valid. |
| Kraken | Unsupported Futures candle streaming creates no physical socket. Empty-cache/admission checks remain valid. |
| Lighter | Recovery no-extra-dial and disposed/guarded fixture traces remain valid. |
| WOO | Disposed open, disposed timer and inert-startup checks remain valid. |
| ApexOmni, BingX, Bitrue, Bitunix, BloFin, BreakoutProp, GRVT, KuCoin, MEXC, Paradex, TxFlow | No remaining lazy-dial fixture expectation found; pending auth/HTTP/preparation, timer, subscription and constructor guards are preserved. |

## Bybit regression and repair

The original named compiled test
`ws_public::tests::dial_tests::disposing_before_dial_is_polled_releases_venue_socket`
fails on master at its pre-poll empty-dial assertion. The repaired fixture covers
public/private × deferred/inline × ready/pending host I/O (eight combinations).
It checks exact production URL/protocols and dial count 1 before any host pump.
A real owner retained inside the pending dial future proves disposal drops the
actual I/O synchronously. It also retains the weak-socket release check before
pumping, verifies no disposed handle, reaps the shared driver after pumping and
requires the complete dial trace to stay unchanged on further pumps.

The shared Promise driver can serve unrelated work: cancellation must drop the
owned operation immediately, while the driver itself is reaped on a later poll.
The fixture no longer mistakes the driver's temporary registration for live I/O.

Independent production controls in Bybit's `scripts/eager-dial-controls.py`
compiled and failed the named witness after (1) changing the actual core dial
boundary to lazy scheduling and (2) leaking the cancelled owned future. Exact
byte restoration passed the same test after each fault. The pending-I/O fault
fails with owner count 2 instead of 1 at `BYBIT-DIAL-CANCEL`; the lazy fault fails
at `BYBIT-EAGER-DIAL`. Logs: `target/bybit-eager-dial-controls/`.

No production source, source mapping, alias guard or identity policy is changed
by this sweep.

## Inherited exceptions

The full Bybit replay exposes two unchanged factory-probe failures:
`api_factory::native_environment_probe_lifecycle` and
`api_factory::source_and_native_factory_surface` both stop at
`tests/support/api_factory_probe.rs:59`,
`factory_probe_native_same_waker_observers`, actual wake count 4 versus expected
3. Running the first compiled integration test alone reproduces the same result.
The integration library excludes this PR's `#[cfg(test)] ws_public::tests`
module; the factory, shared production sources and probe fixtures are unchanged.
Coordinator claude-ddi2il confirmed the inherited exception and routed it to
Bybit owner codex-0w5ohi. Its exact wake-count assertions remain intact.

`corepack yarn parity:extract:check` reports only the inherited Lighter
`transaction-client.ts` manifest drift (0 added, 0 orphaned, 7,563 symbols).
Coordinator routed that separate repair to codex-kxakrb. No extractor policy or
Lighter source changes are included here.

## Checked gates

Normally merged master `b973046b66` before the final gates. All Cargo calls use
the pinned workspace toolchain, `CARGO_INCREMENTAL=0`, `--locked --jobs 2`.
Formatting uses the scoped `cargo fmt -p safe-cex-venue-bybit -- --check`.

| Gate | Measured result |
| --- | --- |
| Original compiled Bybit fixture | Named RED at the old empty-dial expectation |
| Independent core controls | Lazy-prefix RED → exact restore GREEN; cancelled-I/O leak RED → exact restore GREEN |
| Bybit source/native dial comparison | 720 original source cases × both scheduler modes; all original guards retained |
| Full parity index | GREEN: 7,563 rows / 16,342 public Rust items |
| All-features cancellation | GREEN: 6 tests; Node controls 11/11 |
| Binance public-connect target | GREEN: 9/9, including merged #1150's reported fixture |
| Full Bybit all-targets/all-features replay | All 73 executables: 462 passed, only the 2 inherited probe failures above; 0 ignored |
| Scoped fmt | GREEN |
| Strict host Clippy | GREEN: `--all-targets --all-features -- -D warnings` |
| Strict Wasm Clippy | GREEN: `--target wasm32-unknown-unknown --lib --all-features -- -D warnings` |
| Override contract | GREEN: 8/8, including existing no-super/chosen-base rejection controls |
| CI wiring | GREEN: 376/376 after the master merge; no registered test-file additions |

The full venue replay compiled every test executable using Cargo's
`--all-targets --all-features --no-run --message-format=json`, then ran every
artifact from its crate directory with `--include-ignored --test-threads=2`.
This keeps the configured Hyro nested Cargo builds outside a parent build step;
no test was filtered out. The full native source and identity corpus stays enabled.

Local receipts: `/tmp/eager-dial-original-red.log`, `/tmp/eager-dial-sweep-controls.log`,
`/tmp/eager-dial-sweep-final-{index,cancel,cancel-node,binance-public,suite,clippy,wasm,override,wiring}.log`
and `/tmp/eager-dial-sweep-remaining-gates.log`. The optional parity CI job
117784892524 independently reports only the same Lighter extractor drift.
