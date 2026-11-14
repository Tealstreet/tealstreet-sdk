# TxFlow testnet

Read [the official integration rules](https://docs.txflow-testnet.xyz/llms.txt)
first. Use each linked `.md` page for exact fields; the
[complete reference](https://docs.txflow-testnet.xyz/llms-full.txt) contains all pages.

Only `testnet: true` is supported. TxFlow remains development-only and outside
`BETA_VENUES`, preview and production availability.

## Agent onboarding and custody

`createTxflowAgentWithBrowserWallet(owner, {testnet: true})` connects the owner
wallet, generates an agent in the browser, requests the owner's
[`approveAgent`](https://docs.txflow-testnet.xyz/api/exchange/approveAgent.md)
EIP-712 signature, submits it, then verifies
[`approvedAgents`](https://docs.txflow-testnet.xyz/api/info/approvedAgents.md).
For wallets that enforce their connected chain, only the direct-approval domain
uses the documented `signatureChainId` override; the action and all agent writes
still use TxFlow testnet/2511. Fund the owner account before approval. The testnet faucet is `POST /request-fund`
with `{address: owner}` ([networks](https://docs.txflow-testnet.xyz/api.md)).
No builder approval was needed for the builder-free orders in this validation;
builder integration is outside this connector's scope.

The result contains `agentAddress` and `agentPrivateKey`. The consuming browser
application keeps them exactly as its Hyperliquid credentials: `key` is the
agent address, `secret` is the approved private key, `applicationId` is the owner.
This helper neither persists nor recovers keys. Default Tealstreet HL storage
is browser local storage; optional account sync encrypts on the client. This
change does not add or change a persistence system or encryption policy.

The Rust venue holds provider references, never a raw key. Hosted construction
removes signing credentials and uses the browser's provider via `txflow-address`
and `txflow-agent`. The device rebuilds an allowlisted, account/vault-bound
statement; it never signs an opaque server digest. There is no server key
creation, persistence or recovery path.

`revokeTxflowAgentWithBrowserWallet` verifies the label and requests an owner
approval of the zero address under that label. No expiry duration is invented:
missing approval (whether removed, replaced or expired) prevents writes. A
fresh approval read precedes each queued command. The node remains the final
check if revocation races a released write.

## Implemented protocol

Every `/info` and `/exchange` request carries `x-api-version: 2026-08-16`.
Prices/sizes remain strings; numeric asset/order IDs remain integers. The
[signing schema](https://docs.txflow-testnet.xyz/api/signing.md) uses
`TxFlow-Testnet`, version `1`, chain `2511`, and the zero verifying contract.
Canonical MessagePack excludes `type`, preserves the documented field order,
and is followed by the nonce in BE8 and optional vault flag/address. Nonces are
allocated at dispatch, shared by the signing address; signatures are low-S.

| Surface | Authoritative field/endpoint reference |
| --- | --- |
| Plain orders, ≤50 per request; serial command batches | [order.md](https://docs.txflow-testnet.xyz/api/exchange/order.md) |
| Cancel by native ID | [cancel.md](https://docs.txflow-testnet.xyz/api/exchange/cancel.md) |
| Amend price and total size | [modify2.md](https://docs.txflow-testnet.xyz/api/exchange/modify2.md) |
| Leverage | [updateLeverage.md](https://docs.txflow-testnet.xyz/api/exchange/updateLeverage.md) |
| Margin mode | [updateMarginMode.md](https://docs.txflow-testnet.xyz/api/exchange/updateMarginMode.md) |
| Position mode | [updatePositionMode.md](https://docs.txflow-testnet.xyz/api/exchange/updatePositionMode.md) |
| Approval and removal | [approveAgent.md](https://docs.txflow-testnet.xyz/api/exchange/approveAgent.md) |
| Current authorization | [approvedAgents.md](https://docs.txflow-testnet.xyz/api/info/approvedAgents.md) |
| Order identity reconciliation | [frontendOpenOrders.md](https://docs.txflow-testnet.xyz/api/info/frontendOpenOrders.md), [orderUpdates.md](https://docs.txflow-testnet.xyz/api/websocket/orderUpdates.md) |
| Metadata and market filter | [perpMeta.md](https://docs.txflow-testnet.xyz/api/info/perpMeta.md) |
| Public streams, heartbeat, reconnect | [websocket.md](https://docs.txflow-testnet.xyz/api/websocket.md) |
| Limits and fresh nonces on retry | [rate-limits.md](https://docs.txflow-testnet.xyz/api/rate-limits.md) |

`confirm: executed` is mandatory. A consensus-only, pending, malformed or lost
reply remains uncertain. A placement's executed acknowledgement has no order
ID: the engine retains a bounded pending-identity receipt until authoritative
REST/WS reconciliation supplies the native IDs. No caller cloid becomes an oid.
Automatic write replay remains disabled; a caller retry creates a new signed
attempt and nonce. Never replay an uncertain order just because a timeout fired. Each physical
placement batch retains its own bounded identity receipt. Only resolved native
IDs allow a genuine engine-ready callback to admit the next batch, preserving
command order without replaying an uncertain batch.

## Live validation, 2026-11-08

A newly generated disposable testnet owner was funded; a distinct locally generated test-harness
agent was approved and later revoked. The browser helper is covered separately by its
wallet-provider regression. Keys stayed in a mode-600 file outside
all repositories. Captures are represented by hashes and redacted observations
in `../../__tests__/fixtures/txflow/testnet-validation.json`; signing vectors
and source hashes are pinned beside it in `golden-vectors.json`.

- REST metadata, context, book and candles passed. `allMids` and `allAssetCtxs`
  returned 403 over REST; both delivered updates over their documented WS channels.
- Funded account state, webData2, open orders, TP/SL, fills, funding and order
  history passed. `/info` funding is `userFundings`, plural.
- Plain order, in-place modify, cancel, cancelByCloid and all three setting
  actions returned executed success. Reads and `orderUpdates` supplied native IDs.
- `/ws` returned `method: PONG`; `/api-ws` returned `channel: pong`. Candle data
  arrived on `/api-ws`. Reconnection plus resubscription restored updates.
  Unsupported subscriptions were silently ignored despite a live heartbeat;
  a healthy socket alone is not proof that every channel was accepted.
- A reduce-only size of `0.0004` against a `0.0002` BTC position read back capped
  to `0.0002`. Position TP/SL required explicit `m: cross`: partial `0.0001`
  and full-position `0` were accepted and read back separately. Literal
  `Infinity` was rejected. Trigger firing/partial-fill semantics are unproven.
- `useNotionalMargin: true` asset 85 accepted/canceled a size-1 isolated order;
  this does not establish its collateral accounting semantics. These advanced
  flags remain disabled, including reduce-only and conditional support.
- Nonce replay returned 40901. After executed revocation, the listing was empty
  and the next agent write returned 20002. This measures this run, not a general
  revocation-latency guarantee. Final positions, orders and TP/SL were empty.
- No 429 was induced under the low-volume requirement. Published budgets are
  50 info requests/s, 10 exchange requests/s and 10 sockets per service per IP;
  measured limits and automatic retry behavior remain unverified.

NX must add the wallet-agent AccountForm branch, preserve the HL browser storage
path, and enable the new hosted signing formats before this SDK change lands.
Wallet-connect/embedded-wallet UX, rotation and export/recovery UX remain NX work.

### Signing parity fixtures

`agent-action-parity.json` in the shared test fixtures covers every supported
agent action with and without a vault. Its unvaulted order is the published
Signing-page vector; the remaining cases are independent MessagePack/ethers
reference vectors, with their exact action-schema URLs recorded in the file.
The registry test compares canonical preimages, fixed testnet digests, local
signatures and hosted signatures for each case.

### Browser call sequence

1. Connect the owner wallet and obtain its public address.
2. On testnet, request the faucet once with `POST /request-fund`, JSON
   `{ "address": "<owner>" }`. Confirm nonzero collateral through
   [`clearinghouseState`](https://docs.txflow-testnet.xyz/api/info/clearinghouseState.md).
3. Call `createTxflowAgentWithBrowserWallet(owner, { testnet: true,
   agentName: 'Tealstreet' })`. The wallet approves the named agent address;
   it never receives the generated private key as transaction data.
4. Hand `key: result.agentAddress`, `secret: result.agentPrivateKey`,
   `applicationId: owner`, and `testnet: true` to the existing browser account
   pipeline. The application chooses the same persistence/sync mode as HL.
   For hosted execution it sends only public options and signer references.
5. At removal, call `revokeTxflowAgentWithBrowserWallet(owner,
   result.agentAddress, 'Tealstreet', { testnet: true })`, then remove the
   browser credential using the existing account-removal flow.

Never use a production key for these validation steps. The SDK does not add
funding, builder fee, export or recovery UI.
