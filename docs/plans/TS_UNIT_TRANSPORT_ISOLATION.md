# Deterministic TypeScript unit transport

## Problem and scope

The Phemex client-order-ID forwarding fixture supplies an empty positions list
but leaves `fetchRawPositionModeForSymbol` live. The formatter then reaches the
real positions endpoint. General CI job 120166442133 timed out at five seconds;
the duplicate suite passed, demonstrating dependence on the external response.

This change owns test isolation only. It changes no exchange implementation,
timeout, transport adapter, replay reference, package dependency or production
request policy. Start from landing `fa13c9030ed51e6e0ee0ce0bfaf898cfea55f0a5`.

## Plan before implementation

1. Add a deterministic original-RED control to the existing forwarding fixture:
   replace its transport with a local spy and assert that formatting makes no
   request. The existing missing raw-position-mode stub must fail this assertion
   without contacting a venue. Supply that missing fact explicitly to fix it.
2. Put one deny-and-record mechanism in the safe-cex unit-test setup. Cover
   mockable global `fetch`, `XMLHttpRequest.send` (including happy-dom's
   synchronous child-process path), and Node socket connection attempts as the
   HTTP/WebSocket backstop. Denial must never call the original transport.
3. Assert the recorded attempts during cleanup as well as throwing at the call,
   so production catch handlers cannot turn a network-dependent test green.
   Rearm around Vitest mock reset/restore and retain checks across cleanup.
4. Preserve intentional fixture transport narrowly: the existing WEEX reconnect
   test binds an in-process server to `127.0.0.1` and explicitly admits only its
   exact bound port during that fixture, revoking admission in `finally`.
   No external host or blanket localhost exception is permitted.
5. Exercise the shared guard's denial, caught-error accounting, mock lifecycle,
   exact loopback-port admission and revocation. Run the complete safe-cex unit
   suite and repair only explicit missing local fixture facts it exposes.

## Boundaries and verification

Keep the existing real WebSocket implementation available for the admitted local
fixture. Tests can replace transports with explicit local mocks; the guard does
not approve runtime network access. Arbitrary subprocesses and independent worker
runtimes are outside this setup's guarantee; intercepting XHR closes the known
happy-dom synchronous escape specifically.

Original-RED evidence, corrected focused tests, the complete unit suite, scoped
typecheck/lint and independent source/adversarial reviews precede readiness.
If the broad sweep requires substantial fixture rewrites, report the scope and
keep the narrow Phemex repair independently reviewable rather than widening into
exchange behavior changes. No merge may overtake the assigned #1783/#1784 queue.

## Observed fixture causes

The first complete guarded sweep rejected nine cases in eight existing files.
Some constructor/reconcile work was fire-and-forget, so its eventual transport
attempt could be charged to a following case. The captured stack, rather than
that following test's name, identifies the source. All repairs supply local facts
or retire the fixture's pending work; none alters production behavior or timeouts.

| Failing fixture | Original call-site cause | Local repair |
| --- | --- | --- |
| Engine relay binary decoding | The preceding fallback probe left a relay's dynamic constructor pending across mock reset; it could instantiate the real `RelayWebSocket`/`ws` socket in the next case. | Close all returned probe connections after checking the fallback destination. |
| Adapter selection (reported on a following venue alias) | Constructing the Paradex fixture starts `initializeSDK` → `Config.fetchConfig` asynchronously; adapter inspection requires no remote SDK account. | Explicit null local SDK-account initializer, preserving real REST client construction and snapshots. |
| Capability branding (two cases, attributed to BloFin/Paradex) | The same Paradex constructor-time config fetch can spill into the next capability case. | Explicit null local SDK-account initializer; capability assertions unchanged. |
| BinancePM position sign | `ACCOUNT_UPDATE` → `throttledFetchPositions` → `executePositionFetch` → `fetchPositions` adds an unawaited REST reconcile beside the synchronous WS mapper. | Local empty reconcile result; assert it was requested, dispose before publication, drain its microtask. |
| Bitget V2 order payload | `formatCreateOrder` → `ensureAccountTypeDetected` → `_detectAccountType` performs a private account probe. | Explicit already-selected Classic/V2 fixture branch. |
| Hyperliquid spot order wire | `placeLimitOrder` → `getBuilderInfo` → `fetchBuilderFeeApproval` → `checkBuilderFeeApproved` fetches an unrelated wallet approval. | Explicit unapproved builder-fee account fact. |
| Lighter OHLCV reconnect | `onOpen` → `primeTickerRestFallback` → `fetchAndApplyTickerFallback` reads market stats alongside rearming OHLCV timers. | Local empty market-stats response. |
| Lighter channel-less error logging | `onMessage` → missing-price fallback → `fetchAndApplyTickerFallback` reads market stats beside the logging assertion. | Local empty market-stats response. |

The Phemex forwarding control independently proves its missing position-mode
stub with one assertion RED and 32 compatibility controls GREEN before repair.
The guard also checks the shared happy-dom implementation owning `send`, because
each window has its own subclass. A sibling-window synchronous-XHR regression
uses a stubbed subprocess backstop so even its original RED cannot contact a
network. Node-environment tests have no XHR global and use the fetch/socket
boundaries instead.
