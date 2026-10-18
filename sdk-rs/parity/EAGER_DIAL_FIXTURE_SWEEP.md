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
| Coinbase, Crypto.com, Polymarket, Phemex | Constructor/inert capability and no-extra-recovery-chain guards remain valid. |
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

Independent production controls and final gate results are recorded below once
executed. No production source, source mapping, alias guard or identity policy is
changed by this sweep.
