# Remote signing on the Rust engine: cost of a port

Status: **B2 DECIDED (Sam, 2026-10-22)**. Port remote HTTP signing and private
WS auth into Rust as a cutover prerequisite, preserving the existing
sign-request/sign-response wire and keyless `remoteSigner` contract for external
SDK users. Shared local Signer work is not proof of the remote wire/host port.

The owner handoff records at
`.aimux/plans/overseer-handoff-claude-limit.md:72`:

> TS support ends after cutover (team won't maintain both).

> Remote signing must be ported into the Rust engine before TS deletion so open-source users keep an upgrade path.

The application cutover accompanies AWS→Vultr: devices use the exchange server,
which holds exchange API keys in memory for the connection plus resume grace.
This application custody decision does not remove external SDK users' remote
signing requirement. The later handoff at `:84` adds:

> Engine delivery: exchange server (Node, services/exchange-server) consumes a pinned SDK package with wasm engine; browser needs no engine.

> TS fallback retained long-term (frozen).

Active TS support ends after cutover; the retained TS fallback is rollback only,
without dual maintenance. Transitional hosted TS delegates do not satisfy B2.
Preserve that frozen rollback path when deciding deletions. Runtime integration,
remote HTTP/WS proof and rollback reachability remain work, not consequences of
the decision. [Current blocker dispositions](RUST_MASTER_MERGE_BLOCKERS.md).

## Historical costing and source baseline

The remaining inventory, estimates and option sketch were recorded before the
2026-10-22 decision. They explain the earlier TS hedge and cost assumptions;
they do not defer the required port or prescribe today's signer API. Actual
implementation must use the shared dispatch-time [Signer core](RUST_SIGNER.md)
and [prerequisites](RUST_SIGNER_PREREQUISITES.md), with validated request/outcome
and HTTP/WS host integration. Production NX deployments and remote wire support
are not claimed here.

Sources: SDK `origin/feat/rust-rewrite` `4b8212be36`; tealstreet-next
`origin/master` `41e538d78d`, read only. **NX** marks tealstreet-next paths.

## Baseline behavior at the recorded source pins

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

### Rust engine at the historical baseline

- No engine path forwards an HTTP config or WS auth to a `RemoteSigner`. A
  directly built `RustEngineExchange` refuses a keyless remote-signed private
  account at startup (#1498); before that, Coinbase started and then failed inside
  `Credentials::new(key, "")` (`sdk-rs/venues/coinbase/src/lib.rs:68`), and
  Hyperliquid started and rejected every order.
- **One sans-I/O signer exists: Paradex `payloadSigner`.** The engine parks the
  queued HTTP action as a `PendingSignature`, emits
  `Action::SignPayload { request, payload, timeout_ms }` (max 256 pending,
  default 20 s), and resumes with `payloadSigned`/`payloadSignFailed`, re-emitting
  the HTTP request with the remaining timeout (`engine.rs:120,218,785-830,
  1514-1530`; host `host.mjs:404-442`). This is the shape a port generalizes.
- Lighter's `lighterRemoteSigner` is only a read-only flag (no transaction
  signing); Paradex's `remoteSigner` path wraps the transport in TS
  (`paradex.signing.ts`), not the engine.
- **Hosted accounts run TS today (shipped).**
  - #1498: `runsOnRustEngine` keeps every keyless remote-signed account
    (`isKeylessRemoteSigned`) on the TS venue; a directly built engine refuses it
    with `keyless remote-signed accounts run on the TS venue`.
  - #1505: master's TS Coinbase and Orderly are restored as hosted-signing-only
    venues (`exchanges/hostedSigningVenues.ts`), loaded for hosted accounts on
    those default-Rust names; a browser `RustEngineExchange` signs
    `signHttpRequest`/`signWsAuth` through them.
  - #1510: a non-Paradex, non-Lighter `payloadSigner` account also stays on TS.
  - So every hosted account signs exactly as on master, with no
    tealstreet-next change. TxFlow has no hosted users. The engine never sees a
    remote signer.

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

## Historical port option sketch (sans-I/O)

The owner requires the existing exchange-server WebSocketRemoteSigner and
browser signingHost wire to remain unchanged. Prove compatibility through both
halves. The engine APIs below are historical sketches; implement them through
the current shared signer and trusted setup contracts.

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

## Historical options and superseding decision

For Coinbase and Orderly, option (a) from this doc's first version (#1500)
shipped as #1505: restore TS Coinbase/Orderly, about 2 days, instead of porting
those two first at about 9-11. That was a transitional hedge. The owner has now
required the Rust remote-signing port before cutover; the alternatives below
retain their original costing context.

| | Keep hosted TS (shipped) | Port remote signing into the engine | Server-held session keys |
| --- | --- | --- | --- |
| What | Hosted accounts run the TS venue; local accounts run the engine | Shared core plus each hosted venue's signer split (design above) | The server holds a short-lived, scope-limited venue key per session and signs locally on the engine; the browser stops signing |
| Effort | done; upkeep only | ~6-8 days core + per venue: Orderly 1, Coinbase 1.5-2, simple HMAC families 0.5-1 each, cryptocom/weex/backpack 1-1.5 each, hyperliquid/asterdex v3/grvt/apexomni 2-3 each, lighter 3+; ~30-45 total | not costed here: key issuance/rotation, custody and revocation per venue, plus a consent flow; engine side is close to zero (it already signs locally) |
| Risk | Two implementations per hosted venue: hosted users run TS while local users run Rust, so fixes land twice and behaviour can drift; every TS venue class with hosted users stays undeletable; Coinbase/Orderly live copies must stay apart from the frozen replay references | New money-path protocol (parked requests, request-integrity check, outcome mapping, re-signing every resend); needs a hosted soak per venue; the browser still holds every secret | Custody: the server holds signing material; venue support for scoped or expiring keys varies, so not every venue can do it |
| Kept if the plan changes | nothing; deleted on port or session keys | everything if remote signing stays long term; the shared core is thrown away if session keys win | everything; it removes remote signing entirely |
| Historical scope | TS hedge before the new owner decision; does not satisfy B2 | Required Rust remote HTTP/WS path for external SDK users | Application server custody; does not retire the external remote-signer contract |

**Required implementation and proof under the 2026-10-22 decision:**

- Complete remote HTTP and WS auth through the shared dispatch signer, using
  the existing sign-request/sign-response wire; accept keyless `remoteSigner`
  Rust construction for supported external contracts. Local signer migrations
  alone do not close this requirement.
- Prove actual host/worker request correlation, deadlines, re-signing on any
  permitted resend, typed pre-send refusal and post-send unknown outcomes.
  Never auto-replay an unknown write. Keep endpoint/intent validation and
  credential redaction at the shared boundary.
- Prove default application execution on the new server from a pinned SDK/WASM
  package, including connection/resume-grace key lifetime and command routing.
  Browser engine delivery is not required for that application topology;
  external SDK remote providers remain supported.
- End TS feature/support work after cutover. Keep the TS fallback long-term,
  frozen for rollback only, with verified reachability. Historical per-venue
  deletion advice above cannot authorize removing that retained fallback.
