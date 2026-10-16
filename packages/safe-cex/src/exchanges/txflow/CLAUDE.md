# TxFlow Exchange Notes

Read `../CLAUDE.md` first for shared exchange-integration conventions.

## Exchange Information

- Exchange name: `txflow`
- Status: Phase 0 scaffold only. Enabled in local development (`enabledInDev: true`), blocked in preview and production (`enabled: false`, `enableInPreview: false`).
- Mainnet is offline per the TxFlow team. Keep the adapter testnet-only and fail closed for non-testnet startup until TxFlow announces mainnet live and we validate host liveness/signing. Do not flip production or preview availability as part of the testnet phases.

## API Documentation

- Beta API docs: https://docs.txflow-testnet.xyz/api
- LLM docs: https://docs.txflow-testnet.xyz/llms.txt and https://docs.txflow-testnet.xyz/llms-full.txt
- Canonical testnet host: `https://rpc.txflow-testnet.xyz`. One host serves `POST /info`, `POST /exchange`, `POST /account`, testnet `POST /request-fund`, `wss://.../ws`, and `wss://.../api-ws`.
- Docs list mainnet REST as `https://rpc.txflow.com`, but mainnet is currently a future-only constant. Do not hit it from the adapter until the mainnet-live pass.
- Ignore older SDK/default-host drift such as `testnet-api.txflow.net` for this connector; the LLM/API docs make `rpc.txflow-testnet.xyz` the active testnet host. TxRelay is separate (`https://devnet-bridge.txflow.net` testnet, `https://api-bridge.txflow.com` mainnet) and is out of scope for trading.

## Authentication

TxFlow is Hyperliquid-shaped: EIP-712 agent-key signing with a main wallet address plus an approved agent key. Future implementation should store the main wallet address in `applicationId` and the agent private key in `secret`, mirroring Hyperliquid's account model.

Direct EIP-712 actions include `approveAgent`, `tokenSend`, `withdraw3`, `approveBuilderFee`, and `revokeBuilderFee`. Agent-signed MessagePack actions include `order`, `cancel`, `cancelByCloid`, `modify2`, `update*`, `outcomeOperate`, and `claimBuilderRewards`. Testnet domain is `TxFlow-Testnet`, chain ID `2511`; mainnet docs list `TxFlow-Mainnet`, chain ID `869`, but keep it unused while mainnet is offline. `signatureChainId` only overrides the EIP-712 domain chain for direct EIP-712 actions; agent-signed actions ignore it.

Every `/info` and `/exchange` request must send `x-api-version: 2026-08-16`. A 429 on `/exchange` means the action did not execute; retry only after `Retry-After` with a freshly signed nonce.

## WebSocket Notes

- `/ws` carries node streams such as books, trades, order/account updates, `clearinghouseState`, and `webData2`. `/api-ws` carries API-service streams such as `candle` and Outcome async failure updates.
- `/ws` subscribe/unsubscribe frames need the channel `type` both at the top level and inside `subscription`, for example `{"method":"subscribe","type":"orderUpdates","subscription":{"type":"orderUpdates","user":"0x..."}}`. Missing the top-level type is silently ignored.
- `/api-ws` uses `subscription` alone.
- Send `{"method":"ping"}` every 10 seconds on both sockets. `/ws` pongs as `{"method":"PONG"}` and `/api-ws` pongs as `{"channel":"pong"}`. Idle sockets can close around 16 seconds with `1006`.

## Rate Limits

- `/info`: 50 requests per second per IP.
- `/exchange`: 10 requests per second per IP.
- `/ws`: 10 live connections per IP.
- `/api-ws`: 10 live connections per IP.

Prefer WebSocket streams over polling, cache `perpMeta`, and remember hosted/server paths may pool multiple users behind one IP.

## Market Filtering

V1 is crypto perps only. In `perpMeta`, accept `currencyType === 'usdc'`, drop `delisted === true`, and treat `haltTrading === true` as non-tradable. Preserve `useNotionalMargin` in raw `Market.info`; its semantics are still unclear.

## Known Gaps

- REST, WebSocket, market data, private reads, and trading are not implemented in Phase 0.
- Reduce-only auto-cap, TP/SL partial/infinity-stop semantics, and a full error-code table still need live testnet measurement before capabilities are enabled.
- The connector must remain fail-closed until market filtering, public endpoint tagging for hosted mode, and deterministic signing/vector tests are implemented in later phases.
