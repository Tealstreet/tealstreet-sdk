# @tealstreet/exchange-mcp

A stdio MCP server that trades on one exchange through safe-cex. Every exchange gets the same tool names and arguments; tools an adapter leaves unimplemented are not listed.

```sh
EXCHANGE=bybit EXCHANGE_API_KEY=... EXCHANGE_API_SECRET=... npx @tealstreet/exchange-mcp
```

## Environment

| Variable | Value |
|---|---|
| `EXCHANGE` | safe-cex exchange name, such as `bybit` or `bybitdemo`. Required |
| `EXCHANGE_TESTNET` | `true` for the exchange's testnet |
| `EXCHANGE_OPTIONS` | `true` to trade the exchange's options, where it offers them on the same account (`bybit` today). An exchange that trades only options (`binanceoptions`) has them on without it |
| `EXCHANGE_INVERSE` | `true` to trade the exchange's inverse contracts, where it offers them on the same account (`bybit` today) |
| `EXCHANGE_API_KEY` | API key |
| `EXCHANGE_API_SECRET` | API secret, to sign in this process |
| `EXCHANGE_API_PASSPHRASE` | passphrase, on exchanges that use one |
| `EXCHANGE_SIGNER_HEADER` | header that carries the bytes to sign, base64, for a signing proxy that holds the secret |
| `EXCHANGE_SIGNER_SIGNATURE` | value the server sends as the signature, for the proxy to replace |

Set `EXCHANGE_API_SECRET` or the two signer variables. Signer mode needs an adapter that supports `payloadSigner` and covers HMAC-SHA256 exchanges.

## Tools

`get_markets`, `get_tickers`, `get_candles`, `get_balance`, `get_positions`, `get_open_orders`, `get_order_history`, `get_fills`, `get_closed_pnl`, `get_income`, `place_order`, `amend_order`, `cancel_orders`, `cancel_all_orders`, `set_leverage`, `set_position_mode`, `set_margin_type`.

With `EXCHANGE_OPTIONS=true`, or on `binanceoptions`: `get_option_expiries`, `get_option_chain` and `get_account_greeks`. `place_order` takes option symbols and an optional `impliedVolatility` where the venue prices by it (`bybit`; `binanceoptions` refuses it). `get_markets` leaves options out unless they are named, so on `binanceoptions` it lists nothing until you name symbols.

With `EXCHANGE_INVERSE=true`: the inverse markets join every tool. They carry `isInverse`, and their order amounts are USD contracts.

Results with times also carry `<field>Utc` ISO dates, because safe-cex keeps fill and candle times in seconds and the rest in milliseconds.

The server starts the exchange as soon as it connects, and fetches everything else when asked. It opens no public websockets and turns periodic polling off. Starting still fetches one snapshot of positions, orders and balance, and with a local `EXCHANGE_API_SECRET` most adapters also open their private websocket; signer mode does not, because the websocket login needs the secret.

## Develop

```sh
yarn test
yarn build   # dist/index.js, one file with no runtime dependencies
```
