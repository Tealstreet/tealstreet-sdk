# Remote signing on the Rust engine: cost of a port

Status: costing for an owner decision; nothing here is implemented.

**Decision (Sam, 2026-10-21).** Remote signing must stay available in the short
term: hosted exchange-server accounts built with `secret: ''` and a
`RemoteSigner`, signed by the browser, keep working at and after cutover.
Server-held session keys are the likely long-term direction but are not
committed. This doc costs porting remote signing into the engine so that
decision can be made with numbers. Cutover status lives in
[RUST_CUTOVER_CALLERS.md](RUST_CUTOVER_CALLERS.md) (B2).

Sources: SDK `origin/feat/rust-rewrite` `193077a0ae`; tealstreet-next
`origin/master` `41e538d78d`, read only. **NX** marks tealstreet-next paths.

## Today

### Hosted remote signing end to end

1. **Server account.** `initPrivateAccount` builds the account with
   `secret: ''`, `remoteSigner: new WebSocketRemoteSigner(ws, accountName)`,
   `hostedMode`, `privateOnly` and the in-process public source (NX
   `services/exchange-server/src/server.ts:972-1106`, options at `:1015-1034`).
   A fatal signer callback disposes the account (`:981-997`).
2. **SDK on the server.** `safeCexAxiosCreate` installs
   `createRemoteSigningAdapter` as the axios adapter whenever `remoteSigner` is
   set (`safeCexAxiosCreate.ts:53-69`). Every request not tagged
   `markSkipRemoteSigning` is sent **unsigned** as a full axios config (`method`,
   `url`, `baseURL`, `params`, `data`, serialized `headers`) and comes back with
   the browser's `headers`, `url`, `params` and `data`, used verbatim
   (`remoteSigningAdapter.ts:143-243`). Retries go back through the signer
   (`executeThroughDefaultAdapters`, `:86-103`; `exchanges/CLAUDE.md` "A resend
   must be re-signed"). WS auth sends `wsAuth: { toSign, outputFormat,
   extraData? }`, with `toSign` already built on the server (`types.ts:1302-1313`).
3. **Wire.** One session WebSocket carries market data and signing:
   `{t:'sign-req', d}` out, `{t:'sign-res', d:{id, config?, wsAuthSignature?,
   error?}}` back (NX `remote-signer.ts:653`, `server.ts:364-368,1346-1382`).
4. **Browser.** The worker signs HTTP: `signingHost` lazily runs
   `createExchange(name, options-with-real-secret)` and calls
   `exchange.signHttpRequest(request)` after re-syncing `virtualClock` (NX
   `apps/web/src/utils/shim/worker/signingHost.ts:74-199`).
   `BaseExchange.signHttpRequest` replays the config through the venue's own
   axios interceptors with a capture adapter, so the **TS venue's interceptor
   produces the timestamp and signature** (`base.ts:2256-2295`). WS auth goes to
   the main thread, `exchangeAccountController.ts:3020-3041` →
   `signWsAuth` (exhaustive `outputFormat` switch, `base.ts:2309-2350`).
5. **Failure semantics.** `SIGN_TIMEOUT_MS = 15_000`; 10 consecutive failures
   over at least 30 s, or 10 min of silence, are fatal and dispose the account;
   a disconnect suspends signing for a 60 s resume grace (NX
   `remote-signer.ts:8,524-537,567-628,706-764`; `server.ts:599-718`). The
   signer's error texts are treated as **not sent** by `isUnknownOutcome`
   (`utils/unknownSendOutcome.ts:9-13`), which order and chaser code rely on to
   resend safely.

### Rust engine today

- `RustEngineExchange` refuses keyless private accounts at startup outside a
  short list (`rust.exchange.ts:606-621`), and no engine path forwards an HTTP
  config or WS auth to a `RemoteSigner`. Coinbase is on that list but still
  fails, inside `Credentials::new(key, "")` (`sdk-rs/venues/coinbase/src/lib.rs:68`);
  Orderly fails its base58 key check.
- **One sans-I/O signer exists: Paradex `payloadSigner`.** The engine parks the
  queued HTTP action as a `PendingSignature`, emits
  `Action::SignPayload { request, payload, timeout_ms }` (max 256 pending,
  default 20 s), and resumes with `payloadSigned`/`payloadSignFailed`, re-emitting
  the HTTP request with the remaining timeout (`engine.rs:120,218,785-830,
  1514-1530`; host `host.mjs:404-442`). This is the shape a port generalizes.
- Lighter's `lighterRemoteSigner` is only a read-only flag (no transaction
  signing); Paradex's `remoteSigner` path wraps the transport in TS
  (`paradex.signing.ts`), not the engine.
- **B2 hedge (#1498, open at this writing).** `runsOnRustEngine` keeps every keyless remote-signed
  account on the TS venue, and a direct engine refuses it with a clear reason.
  That keeps every **opt-in** venue working through cutover with no port. It
  cannot help the 9 **default-Rust** names, whose TS classes are deleted:
  hosted Coinbase, Coinbase spot and the 6 Orderly brokers fail at startup, and
  the browser cannot sign for them either (`signingHost` builds a
  `RustEngineExchange`, which has no `xhr`). TxFlow has no hosted users.

## Venue signing inventory

HTTP scheme and WS auth as the TS venue signs them on master. "Engine sign
sites" is a raw `sign::` grep in the Rust venue, imports and test modules
included, so it only indicates size (OKX's 1 is an import; Binance's 8 is 3 real
uses).

| Family (names) | HTTP signing | Private WS auth | Engine sign sites | Port shape |
| --- | --- | --- | --- | --- |
| binance (binance, binanceinverse, binancepm, binancespot) | HMAC-SHA256 hex over query, `timestamp` + `recvWindow` | listenKey over REST; spot `hex` | 8 | simple |
| bybit (bybit, bybitinverse, bybitspot, bybitdemo, hyrotrader) | HMAC-SHA256 hex, `X-BAPI-TIMESTAMP` + `recv_window` | `hex` (`expires`) | 10 | simple |
| okx (4 names) | HMAC-SHA256 base64, ISO timestamp + passphrase | `base64` (private + business) | 1 | simple |
| bitget (3 names) | HMAC-SHA256 base64 + passphrase | `base64` | 8 | simple |
| blofin | HMAC-SHA256 hex → base64 + nonce | `hex-base64` | 3 | simple |
| mexc, phemex, woo, bitrue, bingx, kucoin (+spot), cryptocom, weex | HMAC-SHA256 variants | listenKey/token over REST, `hex`, or local secret today (cryptocom, weex private WS need a local secret) | 4-22 each | simple, WS gaps |
| kraken (+spot) | HMAC-SHA512 over SHA256(nonce+body), base64 secret | `kraken-challenge` (venue override) | 14 | simple + custom WS |
| apexomni | HMAC base64 + local Stark seed for orders | `apexomni-base64` | 24 | hard: Stark order signature is local |
| asterdex | v1 HMAC hex; v3 EIP-712 | none | 13 | v1 simple; v3 hard |
| backpack | ed25519 over sorted params + window | local secret today | 9 | medium |
| grvt | EIP-712 order signatures + session | none | 13 | hard |
| hyperliquid (+spot) | EIP-712 action hash; nonce chosen server-side in the body | none | 3 | medium: signature lives in the body |
| coinbase (+spot) | ES256/Ed25519 JWT per request (`uri` claim, 2 min expiry) | `coinbase-jwt` per subscribe | 4 | medium |
| orderly (6 brokers) | ed25519 over `timestamp+method+path+body`, `orderly-timestamp` header | ed25519 over timestamp (TS used the local secret, so hosted private WS was already broken on master) | 2 | simple |
| paradex | Stark via `payloadSigner` (already engine-native) | JWT from `/v1/auth` | 6 | done |
| lighter (2 names) | Schnorr tx signatures, nonce from `nextNonce` | auth token | 11 | hard; read-only today as TS |
| polymarket | EIP-712 with the wallet key in `key` | L2 creds | 13 | n/a (key is local) |
| txflow | local secret | — | 3 | n/a (no hosted users) |
| breakoutprop | DXtrade username/password login | session | — | n/a (no remote signing on master) |

## Port design (sans-I/O)

The port keeps today's wire protocol, so neither exchange-server's
`WebSocketRemoteSigner` nor the browser's `signingHost` changes:

1. **Server engine: external signing mode.** When an account has a remote
   signer, a venue builds each private request **unsigned** and the engine
   parks it as a `PendingSignature` (generalized from Paradex), emitting
   `Action::SignHttp { request, unsigned: {method, url, params, body, headers},
   timeout_ms }`. WS auth emits `Action::SignWsAuth { socket, to_sign,
   output_format }`. The host answers through `remoteSigner.sign({ type: 'http',
   config })` / `{ wsAuth }` and calls `engine.httpSigned(request, {headers, url,
   body})` or `signFailed`. The engine checks the returned request still targets
   the same method and path before sending it.
2. **Browser: engine-local signing.** `RustEngineExchange.signHttpRequest` and
   `signWsAuth` gain an engine command that applies the venue's own signer to a
   supplied unsigned request with the local secret, so a browser
   `createExchange(name, realSecret)` returns a signer for every Rust venue,
   including the default names. This is the piece that removes the need for TS
   classes in the browser.
3. **Timestamps and nonces.** As on master, the **signer** stamps the request:
   the browser engine sets `timestamp`/`recvWindow`/`X-BAPI-TIMESTAMP`/
   `orderly-timestamp` at sign time from its synced clock (`virtualClock`
   re-sync, NX `signingHost.ts:39-72`). Nonce venues (Hyperliquid, Lighter) keep
   choosing the nonce on the server engine and send it inside the unsigned body,
   so ordering stays server-side. A sign can take up to 15 s of the 20 s
   `recvWindow` both stacks use for Binance/Bybit REST (`binance/src/sign.rs:5`).
   The one tight window is Binance spot's user-data WS auth at 5 s (Rust
   `account.rs:2364`, TS `USER_DATA_RECV_WINDOW`), already the case on master.
4. **Timeouts.** The command deadline includes signing; the HTTP request is
   re-emitted with the remaining budget, as Paradex does. Signing work counts
   against `SIGN_TIMEOUT_MS` on the server.
5. **Outcomes.** A sign failure or timeout before the request is sent is
   `outcome: 'notSent'` (safe to resend); after the signed request is sent the
   usual venue outcome applies. This must match `isUnknownOutcome`'s existing
   treatment of signer texts, or chasers would stop resending, or resend after an
   unknown send.
6. **Resends.** Every engine retry of a signed request is re-parked and
   re-signed, never replayed with a stale signature (the TS rule, CLAUDE.md "A
   resend must be re-signed"). Public requests skip signing, the engine
   equivalent of `markSkipRemoteSigning`.

### Security review points

- The server engine must never hold or log the secret: unsigned requests
  only, `Debug` redaction on parked requests, no secret in `SignHttp`.
- The browser signs whatever the server asks. Today that is bounded by the TS
  interceptor's own URL rules; the engine signer must apply the same allowlist
  (Hyperliquid's fund-moving actions are refused, `hyperliquid/CLAUDE.md`), and
  reject requests whose host is not the venue's.
- The returned request is checked against the parked one (method, host, path,
  body hash) so a compromised signer reply cannot redirect a request.
- Replay: venue timestamps/nonces bound replay as today; the 256-pending cap
  bounds memory.

## Effort

Estimates in engineer-days, including tests and review fixes; "simple" means a
signer that is a pure function of the request and a timestamp.

| Item | Days | Notes |
| --- | --- | --- |
| Shared core: generalize `PendingSignature`, `SignHttp`/`SignWsAuth` actions, host wiring to `RemoteSigner`, engine-local sign command, outcome mapping, request-integrity check, tests | 6-8 | Paradex's path is the template |
| Orderly (6 brokers) | 1 | ed25519, 2 sign sites; also fixes hosted private WS |
| Coinbase (+spot) | 1.5-2 | JWT per request and per WS subscribe |
| Simple HMAC families (binance, bybit, okx, bitget, blofin, mexc, phemex, woo, bitrue, bingx, kucoin, kraken) | 0.5-1 each, ~9 total | WS auth formats already enumerated in TS |
| cryptocom, weex, backpack (private WS needs a local secret today) | 1-1.5 each | WS auth path is new |
| hyperliquid (+spot), asterdex v3, grvt, apexomni (Stark) | 2-3 each | signature or seed inside the body |
| lighter | 3+ | tx signing; read-only today on both stacks |
| **Full port** | **~30-45** | plus a hosted soak on exchange-server |

## Coinbase and Orderly: restore TS vs port first

The hedge covers every opt-in venue. Only the default-Rust names need more
before cutover. Two options:

| | (a) Restore TS Coinbase/Orderly as keyless-only venues | (c) Port remote signing for Coinbase and Orderly first |
| --- | --- | --- |
| What | Restore master's TS venue files (their imports all resolve on landing; the `__tests__/reference` copies are not the source, since they carry replay-only patches such as Orderly's ungated broker-suffixed market ids). `createExchange`/`ExchangeWorker` load them for keyless remote-signed accounts on those names through an options-aware loader (`exchanges/hostedSigningVenues.ts`); `RustEngineExchange.signHttpRequest`/`signWsAuth` for those names delegate to a lazily built TS instance, so the browser signs as on master | Shared core (rows 1-2 of the design) plus the Coinbase and Orderly signer splits |
| tealstreet-next change | none | none |
| Effort | ~2-2.5 days (React Native unaffected: the 9 names are not offered on mobile) | ~8.5-11 days |
| Risk | Low: hosted users get master's code path back. Cost: two implementations of these venues until the port or session keys land, and the frozen references must stay separate from the live copies | Moderate: new money-path signing protocol, first shipped on two venues; needs a hosted soak |
| Thrown away later | All of it, when the engine signs remotely or session keys land | Nothing if the full port proceeds; the shared core is discarded if session keys replace remote signing |

**Recommendation: (a).** It is about a quarter of the cost, restores exactly the
behaviour hosted Coinbase and Orderly users had on master, and needs no change
outside the SDK. (c) only pays off if the full port is chosen, which is the
decision this doc exists to inform; doing (a) now does not prejudice it.
