# Bitrue socket recovery

Bitrue is admitted by the SDK beta roster for Client and Hosted construction.
SDK #3502 restores the live venue dropped by #3317 without a product decision;
existing signing, public-mode and credential-placement checks still apply.

Public and private sockets use independent engine `Supervisor` instances with
10-second connect/token deadlines, 20-second receive watchdogs, and capped
exponential reconnect with deterministic jitter. Opening a socket does not
reset an unstable session's failure count.

Heartbeats remain server initiated. Public replies preserve the existing
literal and numeric pong forms; private replies preserve the server timestamp.
Each private recovery POSTs a fresh futures listen key. Session generations
fence late responses and cancel old keepalives. Public reconnects retain
subscription ownership, retire pending socket reads, and invalidate books.

## Standalone signing codecs

Bitrue futures uses HMAC-SHA256, rendered as lowercase hex, over the exact
`timestamp_ms + METHOD + path_and_query + serialized_body` bytes. It has no
signed nonce. The receive window defaults to 20,000 ms and clamps to
1–60,000 ms; `recvWindow` is a header outside the signed payload. The engine
freezes each dispatch timestamp and rejects stale signing completions.

Standalone Client construction and hosted-signing capture use opaque identities
with the same native dispatch recipes; Hosted admission follows the SDK roster.
Only the provider owns the signing material. RemoteSigner
receives the canonical payload through the existing `ws-auth` / `hex` envelope;
no secret or opaque signing ref crosses that wire. Browser `signHttpRequest`
uses a standalone codec backed by native hosted capture, including requests to
the separate `fapiws-auth.bitrue.com` origin. It preserves serialized bodies and
query bytes; absolute request URLs retain their origin through the hosted
signing adapter, while only path/query enter the HMAC. It cannot fall back to a
local key on the server. Callback errors are redacted. Authentication,
receive-window, content-type and broker headers
cannot be replaced by callback metadata.

The signed endpoint inventory is catalogue `GET /fapi/v1/contracts`; account
and positions `GET /fapi/v2/account`; ordinary/protective orders and terminal
proofs `GET/POST /fapi/v2/order`, `POST /fapi/v2/cancel` and
`GET /fapi/v2/openOrders`; leverage reads/writes
`GET /fapi/v2/leverageBracket`, `POST /fapi/v2/level_edit`; and execution history
`GET /fapi/v2/myTrades`. Cancel-all, amendment and protection restore reuse
these native recipes. Public WebSocket traffic remains unsigned.

Private WebSocket authentication is signed HTTP:
`POST https://fapiws-auth.bitrue.com/user_stream/api/v1/listenKey`, followed by
the token-authenticated socket URL above. The 30-minute refresh signs
`PUT` to that same path plus the listen key. RemoteSigner supports this entire
streaming lifecycle; there is no independent socket-login MAC.
Hosted HTTP parity also covers explicit listen-key `DELETE` requests without
changing their authentication origin or path.

PayloadSigner accepts the canonical HMAC payload and selects a primary or
secondary API key explicitly. It follows the shared REST-only account policy:
account/order polling and writes remain enabled, while listen-key acquisition,
private sockets and keepalives are suppressed. Secondary local keys remain
supported. RemoteSigner's raw HMAC envelope has no device slot selector, so
secondary RemoteSigner setup refuses before I/O with `unsupported` / `notSent`
and an actionable explanation. This adapter delegates API-key custody; it does
not introduce an agent-wallet or EIP-712 signing mode.

`construction-rules.json` declares `remoteSigning.enabled`, `payloadSupported`
and `payloadRestOnly` for both admitted accounts and standalone codecs. Bitrue
uses the default admitted construction rule and the WASM beta loader ships its
family. NX's derived refusal list follows the SDK declaration on import.

Independent HMAC vectors cover every signed route plus listen-key creation and
refresh. The native fixture provider compares actual dispatched graphs and
signature bytes with hosted capture across the existing account, history,
ordinary/protective trading, recovery and failure tests. Actual Wasm tests
compare local/device and keyless/server wires at fixed and corrected clocks,
exercise private startup and REST-only startup, and reject malformed or failed
callbacks without network or local fallback. These tests use synthetic fixture
credentials and no live exchange calls.

## Legacy TS allocation parity

The splitter emits equal rounded lots plus a separately rounded modulo tail,
matching `packages/safe-cex/src/__tests__/reference/bitrue/bitrue.exchange.ts:1180-1196`.
At amount 26, maximum 5 and step 1 this sends `[4, 4, 4, 4, 4, 4, 1]`, sum 25.
KNOWN-ISSUE: legacy TS under-allocates (sum 25 of requested 26); kept for exact TS parity per Sam's decision
Rounding can also over-allocate: `0.025 / 0.01 / 0.001` sends
`[0.007, 0.007, 0.007, 0.005]`, sum `0.026`. No conservation correction is applied.
Generated below-minimum tails reach the venue rather than causing a local
split refusal, and there is no local thousand-lot gate. Existing limit/market
cap selection, quote conversion, command dispatch and guarded protective
put-back rules stay unchanged.
