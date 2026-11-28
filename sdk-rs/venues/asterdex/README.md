# AsterDEX socket recovery

Public and private sockets use the engine `Supervisor` for a 10-second connect
and token-acquisition deadline, a 20-second receive watchdog, and capped
exponential reconnect with deterministic jitter. Opening a connection does not
reset failures; a stable session does. Both sockets retain their native
`LIST_SUBSCRIPTIONS` heartbeat every five seconds.

Private recovery obtains a new listen key. Retiring a session cancels its
30-minute keepalive and invalidates pending key and keepalive responses by
session epoch, so an old failure cannot close a replacement socket. Account
reconciliation and retained public subscriptions keep their existing ownership.

## Hosted and delegated signing

V1 HMAC and V3 API-wallet accounts support RemoteSigner without an origin
secret. V3 resolves from the public main-wallet (`auth.userAddress` or
`applicationId`) and API-wallet (`auth.signerAddress` or `key`) addresses, or an
explicit `extra.asterdexApiVersion`. The API wallet is Aster's delegated agent;
the main wallet remains the account owner. Secondary read agents use the
device's secondary slot. Address setup proves each signing wallet before the
engine allocates its address-bound, monotonic microsecond nonces.

The device wire has three formats: `asterdex-address` selects a public wallet;
`asterdex-hmac` signs exact V1 query bytes; `asterdex-wallet` carries a closed V3
request statement. Both request formats select primary or secondary custody
and return the public credential plus signature atomically. Opaque refs and
private keys never cross this wire. V3 statements bind method/path, ordered
business parameters, main wallet, API wallet, nonce, network and clock. The
device rejects foreign accounts/networks, unsupported endpoints, unknown fields
and clock differences over 30 seconds, then derives EIP-712 `Message.msg`
digests itself for every V3 request. Native apply verifies the statement commitment and recovered
signer before releasing the exact local wire signature.

PayloadSigner supports V1 canonical HMAC payloads with REST-only lifecycle.
The native account loads and polls balances, positions and orders, including
fresh position commands, without registering a private feed or creating,
renewing or reconnecting a listen-key session. WS signing stays refused.
Private V3 PayloadSigner is `unsupported` / `notSent`: its HMAC-only callback cannot
derive account-bound wallet signatures or prove the wallet address. It requires
RemoteSigner wallet custody instead. Explicit public-only construction ignores
delegated callbacks and credentials for both API versions and retains no signing authority.
Generic HTTP signing captures remain V1
only; V3 native REST requests use the structured wallet protocol above. Aster
has no independent WS login signature: signed listen-key creation/keepalive
authenticates the private socket. A generic WS HMAC login remains refused.

| Signed operation     | Endpoints, relative to `/fapi/v1` or `/fapi/v3`                                                                           |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------- |
| Account and reads    | GET `account` (V1 uses `/fapi/v4/account`), `positionRisk`, `openOrders`, `order`, `leverageBracket`, `positionSide/dual` |
| History              | GET `allOrders`, `userTrades`, `income`                                                                                   |
| Trading and settings | POST `order`, `leverage`, `marginType`, `positionSide/dual`; DELETE `order`, `allOpenOrders`                              |
| V3 controls          | GET/POST `stpMode`; GET `builder`, `agent`                                                                                |
| Private WS session   | POST/PUT `listenKey`                                                                                                      |

Amend uses cancel, order proof and replacement placement. V1 requests sign a
fixed 20,000 ms receive window. Every authenticated V3 request signs the exact
query as a network-specific EIP-712 `Message.msg`, with a microsecond nonce and
no timestamp/recvWindow fields. The old endpoint-specific EIP-191 branch is retired.
The current API permits nonce times within 60 seconds of server time; engine
nonce custody and the stricter device clock guard remain unchanged. Clock recovery rebuilds and
re-signs a request once with a fresh stamp/nonce. Ambiguous released writes
remain Unknown without replay; callback refusal before release is NotSent.

V1 private startup GET reads retain their existing single re-signed resend for
`-1022`. During unfinished primary-key startup, a final numeric or string-coded
rejection reports `AuthInvalid` through the existing startup guard, matching
legacy credential validation. A successful resend can finish
the read. Timestamp recovery and trading-write retries retain their existing rules.
A V3 account signature refusal terminates unfinished credential validation.
Aster removed the V1 futures API from its docs on 2026-11-02 and now answers the
V1 order routes (`openOrders`, GET/POST/DELETE `order`, `batchOrders`,
`allOpenOrders`) with an HTML 404, while V1 `account`, `positionRisk` and
settings routes still answer. An envelope-less 404 on the V1 `openOrders`
startup read therefore reports `AuthInvalid` at once, naming the V3 API wallet
key as the replacement, instead of retrying until the host's start deadline
(Sentry TEALSTREET-NEXT-C0W/EGX). A 404 on any other V1 read, whose route
still serves, and every V3 404 keep their ordinary retry.
Known signature refusals (`-1022`, or `-1000` with `Signature check failed`)
report `Aster API key signature rejected: re-create the key (V3)`. Position-mode
refusal is terminal for that account configuration: no retry timer is armed,
and placement remains held because its mode is still unknown. Listen-key POST/PUT failures retain their independent socket
recovery and do not terminate pending credential validation.

Native fixture custody compares every real signing graph with the device codec,
covering reads, ordinary/protective writes, cancel/amend, listen-key lifecycle,
clock recovery, refusal and Unknown. The reducer's synthetic `/probe` is outside
the device endpoint allowlist and retains its local control. Independent ethers
vectors pin V3 EIP-712 signatures on both networks; actual WASM fixtures exercise V1 HTTP parity,
V3 vectors, keyless startup/WS, secondary custody and redacted failure paths.

# Legacy write parity

Split lots, batch rows and separate entry/SL/TP children retain the legacy
TypeScript driver's partial placement behavior. Rejected elements do not stop
later writes. Caller client IDs are checked across the batch before I/O, while
algo capacity is checked per intent without a cumulative batch reservation.
Cancellation batches continue after a refusal; a refused single cancel in an
amend prevents its replacement. Accepted placement IDs survive partial failures.

The shared write-plan types report indexed placed, applied, refused and uncertain
outcomes on terminal failures. They do not gate payloads or decide continuation.
Engine-owned transport ambiguity still seals writes under its existing custody
rules. No new validation or retry policy is introduced here.

Known legacy limitations remain: a split tail or later row can be refused after
an earlier write takes effect, and an entry can succeed without both protective
children. A replacement can fail after its original order was canceled.

The class-A restore kept pending policy is the inherited Rust exception: after
a confirmed protective cancel and a definite replacement refusal, its existing
single put-back is unchanged. Legacy TS `updateOrder` has no corresponding
restore; this exception follows the lead's explicit HOLD/no-removal instruction.

Evidence: `packages/safe-cex/src/__tests__/reference/asterdex/asterdex.exchange.ts`
lines 742–867 (per-intent checks and splitting), 955–997 (placement and batch
continuation), and 1000–1081 (cancel-replace and cancellation fan-out).

Stop-after-Unknown is a recorded TS parity exception (Sean/daxx, 2026-11-08).
An uncertain row ends the action with its placed/applied prefix, Unknown row
and unsent suffix in the journal, standard Check Open Orders guidance and a
`write.unknown` event for venue-decoded failures. Telemetry for engine-sealed
Unknown is pending an engine hook in a later engine PR; halt, canonical engine
wording and journal coverage remain pinned. Definite refusals keep the TS
continuation behavior.

## Stop-entry brackets (AGT-115)

`plan_place_inner` builds stop-loss/take-profit legs only after a limit/market entry, as
the TS reference does. Any other entry with a non-zero attached leg used to be placed with
its legs cleared; the whole command now refuses before any write with the engine's shared `stop_entry_legs_refusal` copy.
