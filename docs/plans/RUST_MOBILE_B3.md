# Mobile (B3): what the device sends, and how to cut it over

Status: decision aid for owner decision 3; nothing here is implemented.

**Question.** tealstreet-mobile runs React Native on Hermes, which has no
`WebAssembly`, so the wasm engine cannot run on its JS thread. Cutover blocker
B3 in [RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md) asks what mobile does
instead. This doc inventories every request the app sends from the device
rather than through exchange-server, and costs three answers: (a) route those
through exchange-server, (b) run the Rust engine on the device, (c) keep TS
venues on the device. Companion to [RUST_REMOTE_SIGNING.md](RUST_REMOTE_SIGNING.md)
(B2), whose decision it depends on.

Sources: SDK `origin/feat/rust-rewrite` `f98a20e7fe`; tealstreet-next
`origin/master` `1f19925437` (**NX**); Tealstreet/tealstreet-mobile `master`
`8377a4f` (**MOB**), read through a shallow clone and the GitHub API. The app
copies tealstreet-next `packages/*` and parts of `apps/web` into gitignored
`src/packages`/`src/web` (`yarn sync`), so synced code is cited from NX. Nothing
was run on a device.

## Live production issues found

These were found while preparing this doc. They affect mobile users today,
independently of the Rust cutover, and none needs B3 decided to fix.

1. **Bitget, BloFin and Binance inverse accounts cannot be built on the
   phone's JS thread (traced in code, not run on a device).**
   - Since NX `ec966018a1` (2026-10-09), safe-cex resolves its transport with
     `options.transport ?? defaultTransport ?? directTransport`
     (NX `packages/safe-cex/src/transport/registry.ts`). Only these register a
     default: `instrumentation-client.ts` (Next.js), `ExchangeServerWorker.ts`
     and `StoreWorkerEntry.ts` (web workers), plus the CLI, exchange-server,
     candle-server and scripts. No other file imports `registerTealstreetTransport`
     or `registerTealstreetRealm`.
   - On mobile, sync copies `apps/web/src`, but the Hermes thread imports none
     of those three. Mobile's own `src/web/utils/shim/worker/workerFactories.ts`
     replaces web's worker setup, and `ExchangeServerWorker` runs only inside
     the WebView. No tracked mobile file calls `setDefaultTransport` or passes
     `ExchangeOptions.transport` (MOB `8377a4f`). So the Hermes mirror runs on
     `directTransport`.
   - `createExchangeFromAccount.ts:66-69` forces `tealstreetProxy: true` for
     every `requiresCors` venue. On mobile those are bitget, blofin and
     binanceinverse (NX `exchangeMetadata.ts` `BitgetMetadata`, `BlofinMetadata`,
     `BinanceInverseMetadata`).
   - `directTransport.rest` throws when `tealstreetProxy` or `tealstreetTunnel`
     is set (NX `transport/direct.ts:15`), and `safeCexAxiosCreate` calls it
     while building the venue's REST client (NX `safeCexAxiosCreate.ts:280`). The
     mirror for those accounts therefore throws at construction. On Hermes that
     means no order writes, warm-start reads or add-account validation for
     them. Any other account the user set to Tealstreet proxy or tunnel fails
     the same way.
   - Hosted streaming is unaffected, because the WebView Worker does register
     the transport.
   - **Which builds:** any build synced from tealstreet-next at or after
     2026-10-09. The synced tree is gitignored and the repo records no source
     revision, so whether the current store or OTA build includes it is unknown.
   - **To confirm:** open a Bitget account on a current build, or check the
     shipped bundle for `registerTealstreetTransport`.
   - **Fix (outside the SDK):** import `@tealstreet/safe-cex/src/transport/registerTealstreetTransport`
     once at the mobile app entry, as web's `registerTealstreetRealm.ts` does.
2. **Lighter orders fail on mobile.** Mobile's Metro config replaces
   poseidon-crypto with a stub that throws ("Lighter DEX features are
   unavailable", MOB `src/mobile/shims/poseidon-crypto-stub.js`). Lighter's
   signer needs it (NX `lighter.signer.ts:4`, `lighter.exchange.ts:737-740`), so
   every Lighter order write from the phone fails, although Lighter is offered
   on mobile (`supportedOnMobile: true`). Fix: route Lighter writes through
   exchange-server (option (a)), ship a Hermes-safe signer, or hide Lighter on
   mobile until one exists.
3. **Venue API secrets are stored in plaintext on the phone.** Keys and secrets
   are in AsyncStorage behind a localStorage shim (MOB
   `polyfills/localStorage-shim.js`; `syncKeyVaultNative.ts:12-15` says so). The
   cloud-sync key is also raw bytes in AsyncStorage, and SecureStore/Keychain is
   not used. At runtime the secrets also live in the WebView Worker's
   `signingOptions` (NX `signingHost.ts:80-100`). Fix: move the secrets and the
   sync key into SecureStore/Keychain (an Expo module, so a native build);
   server-held session keys would remove them from the device entirely.

## Today

### How the app talks to exchanges

1. **Hosted mode only.** The mobile-patched `shouldUseExchangeServer()` always
   returns true (MOB `src/web/utils/shim/worker/workerFactories.ts:5,173-176`).
   No exchange runs `start()` on the Hermes thread.
2. **A hidden WebView owns the session.** `ExchangeHostWebView.tsx:126` loads
   `/exchange-host` (MOB `exchangeHostBridge.ts:25`). That page refuses to run
   outside `window.ReactNativeWebView` and starts `ExchangeServerWorker` in a
   real Worker (NX `apps/web/src/app/exchange-host/page.tsx:33-53`). Its only
   socket is `wss://connect.tealstreet.io/account` (NX `ExchangeServerWorker.ts:146-148`).
   The WebView exists because signing on the RN JS thread measured 107 ms
   average, 746 ms worst, against 2 ms in a Worker (NX `signingHost.ts:13-15`).
3. **The device signs for the server.** exchange-server builds every account
   with `secret: ''` and a remote signer (B2). HTTP `sign-req` frames are signed
   inside the WebView Worker with the venue secret (NX `ExchangeServerWorker.ts:879-920`,
   `signingHost.ts:88-191`). WS-auth requests, and any HTTP request the Worker
   cannot sign, go to the Hermes mirror (below) (`ExchangeServerWorker.ts:925-933`,
   NX `exchangeAccountController.ts:3020-3041`). Secrets are never sent to
   the server; only allowlisted fields are (`ExchangeServerWorker.ts:190-218`).
4. **A Hermes "mirror" per account trades directly.** `createExchangeFromAccount`
   (NX `exchangeAccountController.ts:1752`) builds a never-started exchange
   through `registry.native.ts`. It holds the secret, and **every order write
   goes from the device straight to the venue, signed on the device**: place,
   cancel, update (NX `apps/web/src/order/CLAUDE.md:9-17`,
   `controllers/CLAUDE.md:41-64`; mobile screens call `@web/order/createMarketOrder`,
   `createLimitOrder`, `createStopLossOrder`, `cancelOrders`). Leverage, margin,
   position mode, chasers, TWAP, candles and history go to the server through
   `callMethodInWorker` (NX `order/setLeverage.ts:19`; `ExchangeServerWorker.ts:1140`;
   `controllers/CLAUDE.md:50-51` still says leverage runs on the mirror, which the
   code no longer does). **Lighter cannot sign there:** mobile's Metro config
   replaces poseidon-crypto with a stub that throws ("Lighter DEX features are
   unavailable", MOB `src/mobile/shims/poseidon-crypto-stub.js`), and Lighter's
   signer needs it (NX `lighter.signer.ts:4`, `lighter.exchange.ts:737-740`), so
   Lighter order writes from the mirror fail today.
5. **Keys live in plaintext.** API keys and secrets are in AsyncStorage behind
   a localStorage shim (MOB `polyfills/localStorage-shim.js`,
   `syncKeyVaultNative.ts:12-15`); SecureStore/Keychain is not used. Cloud sync
   stores one client-encrypted blob (NX `docs/accounts/CLOUD-SYNC-v1.md:198-238`).
   At runtime the secrets are in two places: Hermes memory (the mirror) and the
   WebView Worker's `signingOptions` (NX `signingHost.ts:80-100`), inside a page
   served from the web origin.

### Mobile venues and the engine

| | |
| --- | --- |
| Mobile venues (`supportedOnMobile: true`, NX `isExchangeAvailableInCurrentEnv.ts:21-41` gates the rest) | bybit, bybitdemo, bybitspot, bybitinverse, binance, binancepm, binancespot, bitget, blofin, okx, hyperliquid, kraken, lighter (landing; master also binanceinverse) |
| Their registry on landing | all TS in `registry.native.ts`; each also has a Rust venue |
| Default-Rust names (`registry.native.ts:34-35,56-64`) | coinbase, coinbasespot, 6 Orderly brokers, txflow: all `supportedOnMobile: false`, so no mobile user runs them |
| On-device engine host | none: `sdk-rs/hosts/` has only `wasm` and `native` (a tokio/reqwest server host). `@tealstreet/safe-cex-react-native` exports `setReactNativeRustEngineLoader`, which needs a host-supplied engine (`packages/safe-cex-react-native/src/rustEngine.ts:3-21`) |
| Bundling | the app bundle breaks once synced: `rust.exchange.ts` imports `@tealstreet/safe-cex-wasm/{host,wire,engine}`, absent from `src/packages`; Metro has no package `exports` and no `.wasm` handling (RUST_CUTOVER_CALLERS "Mobile") |

So **no Rust venue runs on mobile at cutover** under any option; the decision
is about the bundle, about which code holds and uses the secrets, and about the
path to retiring the TS venues.

## Inventory: requests the device sends itself

Everything not listed here goes through the hosted session socket.

| Flow | Where | Target | Venue | Signed (key) | Read/write |
| --- | --- | --- | --- | --- | --- |
| **Place / cancel / update orders** | NX `order/*`; MOB order screens | venue REST, or a Tealstreet proxy (`*.jp`/`*.asia.tealstreet.io`) when `tealstreetProxy` is set | every mobile venue except Lighter (its signer is stubbed out on Hermes, so writes fail) | venue secret (HMAC; EIP-712 wallet key on Hyperliquid) on Hermes | **write** |
| Hyperliquid builder-fee approval | NX `HyperliquidBuilderFee.tsx:8-13` → `hyperliquid.builderFee` | Hyperliquid exchange API | hyperliquid | wallet key on Hermes | **write** |
| Add-account validation (`validateAccount`) | MOB `AccountForm.tsx:10,215` → NX `useAccountForm` | venue REST | every mobile venue | venue secret on Hermes | read |
| Warm-start reads (`fetchMarkets`, `fetchOrders`, `fetchPositions`) | NX `eagerExchangeWarmStart.ts:58,75,86` via `exchangeAccountController.ts:1755` | venue REST | every mobile venue | venue secret on Hermes | read |
| Recovery reads (empty-ticker refetch; Phemex `fetchPositions`) | NX `exchangeAccountController.ts:666-697,863` | venue REST | tickers: any; positions: Phemex | public / venue secret | read |
| HTTP `sign-req` answers | NX `ExchangeServerWorker.ts:900-903` | over the hosted socket | every venue | venue secret in the WebView Worker | signs the server's reads and writes |
| WS-auth `sign-req` answers | NX `exchangeAccountController.ts:3029` | over the hosted socket | venues with private WS auth | venue secret on Hermes | auth |
| Wallet signing | MOB `wallet/walletConnect.ts:100` | WalletConnect relay | wallet venues | `eth_signTypedData_v4` in the user's wallet | write |
| Candles | NX `cachedCandles.ts:28,66` | `data.tealstreet.io/candles/api/market-data/candles` | public | none | read |
| Tealstreet API (`/api/proxykey`, `/api/settings`, `/api/synced-exchanges`, `/api/auth-twofa`, marketplace, `/api/time`) | MOB `proxyToken.ts:71`, `api.ts:26-65`, settings screens | `preview.tealstreet.io/api/*` | — | Auth0 Bearer | read/write |
| Auth, news, push, OTA, error reports | MOB `auth0-native.tsx`, `MobileNewsFeedsWorker`, `notificationsApi.ts`, `pushRegistration`, `app.config.js` | Auth0, `news.tealstreet.io`, Expo, Sentry | — | OAuth / proxy token | read/write |

The first five rows touch a venue; the order writes and the Hyperliquid
builder-fee approval are writes. Those are what B3 is about. Listen keys and
private WS run server-side through `sign-req`; the device opens no public venue
socket. The Hermes mirror runs without a registered transport, so bitget,
blofin and binanceinverse accounts cannot be built there (issue 1 above).

## Options

### (a) Route the direct writes and reads through exchange-server

The Hermes mirror forwards order writes and warm-start/recovery reads to the
hosted session with `callMethod`, as #1443/#1462 already do for Rust mirrors on
web. exchange-server executes them and signs through the existing `sign-req`
path, so the device keeps signing but no longer sends anything to a venue.

- **Work:**
  - SDK: let a TS mirror in hosted mode forward the B4 trading methods and the
    warm-start reads (generalize the `RustEngineExchange` forwarder into
    `BaseExchange`, gated on a hosted mirror).
  - tealstreet-next: the warm-start skip for hosted mirrors (already a listed
    cutover edit).
  - Mobile: nothing beyond `yarn sync`.
- **Lost-send semantics (the hard part):** a forwarded call on a TS mirror
  uses the base 5 s timeout and a plain `Timeout callMethodInWorker` error
  (`base.ts:1755-1759`), while the server waits up to 15 s per signature
  (NX `remote-signer.ts:8`) and one order can need several. The device would
  report failure while the server still places the order, and a retry
  duplicates it. TS forwarding needs what Rust has: `COMMAND_TIMEOUT` with
  `clientOrderIds`, `outcome: 'unknown'` and a reconcile step
  (`rust.exchange.ts:888-898`). The web optimistic-order-id and
  `resolveOrderId` flows also assume a local placement. Warm-start reads are
  simpler to skip in hosted mode than to forward.
- **Effort:** SDK forwarding with timeout/outcome parity 1-1.5 weeks; web order
  flows and a hosted mobile soak 1 week; about 2-2.5 weeks. JavaScript only, so
  it ships OTA (SDK and web through `yarn sync`).
- **Latency:** an order write becomes device → server → device signs → server →
  venue, instead of device → venue. The sign round trip has measured 819 ms+
  against an 11-93 ms ping, all inside the client (NX `remote-signer.ts:466-470`);
  that figure was on the browser main thread, and the Worker signer measured
  2 ms (NX `signingHost.ts:13-15`), so it should be far less, but it must be
  measured before committing. No hosted account pays this today: hosted
  accounts run TS, whose mirrors send orders directly (#1498).
- **Security:** fewer client-to-venue paths, and the whitelisted-IP egress moves
  to the server, but the device still holds and uses every secret.

### (b) Rust engine on the device for the direct requests

Ship a native engine host: a Rust static library for iOS/Android exposed to JS
through JSI or uniffi, implementing the `EngineHost` transport callbacks, plus
an Expo module. The mirror then runs `RustEngineExchange` with the native loader.

- **Work:** a new `sdk-rs/hosts/mobile` crate; the JSI/uniffi binding and its
  transport (JS fetch/WebSocket or native reqwest); an Expo config plugin; EAS
  native builds for both platforms; a device acceptance gate; app-size and
  startup budgets.
- **Effort:** about 5-8 weeks, and every engine change then needs a native app
  release rather than OTA.
- **Latency:** same as today for writes (device → venue), with native signing.
- **Security:** the same custody as today, plus new native code on the money
  path.
- **Variant:** run the engine as wasm inside the existing exchange-host WebView,
  which has `WebAssembly`, and send the mirror's writes there. Cheaper (about
  1-2 weeks) and it ships with a web deploy (`/exchange-host` is served from the
  web origin), but it needs the same lost-send work as (a), is a second hop
  inside the device, and converges on (a) anyway.

### (c) Keep TS venues on the device

Leave the 13 mobile venues on TS in `registry.native.ts`, as on master; no
default-Rust name is offered on mobile.

- **Work:** fix the bundle. Make `registry.native.ts` not reach `rust.exchange.ts`
  on React Native (a Metro stub like the existing paradex-sdk/poseidon ones, or
  a native registry with no Rust loaders), and smoke the app.
- **Effort:** about 0.5-1 day. OTA.
- **Cost:** those 13 TS venue classes cannot be deleted while mobile ships, and
  fixes land twice (Rust for web, TS for mobile). The same constraint B2's
  hosted-TS hedge already imposes on hosted users.
- **Security and latency:** unchanged from master.

## Interaction with session keys

Server-held session keys (the likely long-term B2 direction, not committed)
would let exchange-server sign locally on the engine without the device.

- **With (a)** they finish the job: the device stops holding venue secrets at
  all, the plaintext AsyncStorage keys and the WebView signer go away, and every
  mobile venue runs on the server's Rust engine. (a) is the routing half of that
  end state.
- **(b)** works against it: it invests in on-device signing that session keys
  would remove.
- **(c)** is neutral: a stopgap that either direction later replaces.

## Effort and trade-offs

| | (a) Route through exchange-server | (b) Engine on device | (c) Keep TS on device |
| --- | --- | --- | --- |
| Effort | ~2-2.5 weeks, OTA | ~5-8 weeks, native releases (WebView-wasm variant ~1-2 weeks, web deploy) | ~0.5-1 day, OTA |
| Order latency | +1 server hop and a sign round trip | as today | as today |
| Secrets on device | yes (until session keys) | yes | yes |
| Device → venue traffic | none | as today | as today |
| Lets TS venues be deleted | yes, once hosted runs Rust for those venues (B2) | yes | no |
| Fits session keys | yes; it is the routing half | no | neutral |
| Main risk | lost-send duplicates unless timeouts match the server's; order latency | native code on the money path; release cadence | TS/Rust divergence for mobile users; Lighter writes stay broken |

## Recommendation

- **For cutover: (c).** It is under a day and fixes the one hard break (the
  Metro bundle). It keeps mobile on master's behaviour, and B3 stops blocking
  cutover. Fix the live issues at the top of this doc regardless of the option
  chosen; the transport registration (issue 1) is a one-line app change.
- **As the direction: (a), tied to session keys.** Route mobile's direct writes
  through exchange-server once the order-latency cost is measured and TS
  forwarding has Rust's timeout and outcome semantics. If session
  keys land, mobile drops its venue secrets entirely, which also removes the
  plaintext-AsyncStorage exposure.
- **Not (b)** unless there is a requirement for on-device execution without the
  server (offline, or latency that (a) measurably cannot meet). Its cost is
  weeks plus native release cadence, and session keys would strand it.
