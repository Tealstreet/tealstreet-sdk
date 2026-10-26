# Bitget USDC replay provenance

These are **synthetic, documented Classic USDC-M fixtures**. They are authored
directly for `bitgetusdc`, not captured from a live account, recorded by driving
Rust, or produced by replacing USDT names in a captured tape. They do not prove
live private behavior, UTA parity, recovery under faults, or performance on a
production-sized market catalog.

The native selectors are `productType=usdc-futures`, collateral `USDC`, WebSocket
`instType: "USDC-FUTURES"`, and symbols `BTCUSDC` / `ETHUSDC`. Both implementations
run their existing venue implementations; no USDC reference-output fixer is used.

| Artifact | SHA-256 |
| --- | --- |
| Generator `../private-tape-bitgetusdc.mjs` | `232137fc29a4233881ccb91a21b903ad4a2a95dc87d29aabe36e24c23d40ad67` |
| Committed `bitgetusdc-public.jsonl.gz` | `107a95ed6dd933afdb739cbd2a68a853e921f6319e59075a2c46fd7a17dd3cb1` |
| Canonical generated private JSONL | `88f8f994e249dbb96eaeb9390d4703bf9761ac4cb9b1385f5815115aa01f9928` |

Each tape's metadata repeats its synthetic provenance, generator, source URLs,
product, collateral, and scope. The epoch is the fixed `1760880000000`.

## Schema sources

The fixtures use the Classic v2 contract API and WebSocket schemas from Bitget:

- [Contracts, tickers and market data](https://www.bitget.com/docs/catalog/classic-contract-market/classic-contract-market).
- [Accounts and collateral](https://www.bitget.com/docs/catalog/classic-contract-account/classic-contract-account).
- [Positions](https://www.bitget.com/docs/catalog/classic-contract-position/classic-contract-position).
- [Orders, amendments, cancellation and fills](https://www.bitget.com/docs/catalog/classic-contract-trade/classic-contract-trade).
- [Ticker messages](https://www.bitget.com/docs/classic/websocket/contract/public/Tickers-Channel).
- [Depth messages](https://www.bitget.com/docs/classic/websocket/contract/public/Order-Book-Channel).
- [Private order messages](https://www.bitget.com/docs/classic/websocket/contract/private/Order-Channel).

Current depth messages use `seq` / `pseq`; the May 2026 protocol change removed
`checksum`. This fixture supplies contiguous sequences and an independently
checked legacy CRC32 as compatibility coverage. It does not claim that a live
October 2026 feed still includes the legacy field. See the
[Classic change log](https://www.bitget.com/zh-CN/legacy-docs/classic/changelog).

## Builders and reproduction

`../private-tape-bitgetusdc.mjs` exports `buildBitgetUsdcPublicTape()` and
`buildBitgetUsdcPrivateTape()`. They return fresh record arrays without writing
files or accessing a network. It also exports `BITGET_USDC_EPOCH`,
`BITGET_USDC_SYMBOLS`, `BITGET_USDC_SOURCES`, and `bitgetUsdcChecksum(bids, asks)`.

From the repository root:

```sh
corepack yarn node sdk-rs/harness/private-tape-bitgetusdc.mjs --public --out sdk-rs/harness/fixtures/bitgetusdc-public.jsonl.gz
corepack yarn node sdk-rs/harness/private-tape-bitgetusdc.mjs --out /tmp/bitgetusdc-private.jsonl
corepack yarn node --test sdk-rs/harness/bitgetusdc.test.mjs
```

`--public` selects the public builder; omitting it selects private. `--out`
accepts a plain JSONL or `.gz` path. Default paths are
`sdk-rs/harness/tapes/bitgetusdc-public.jsonl` and
`sdk-rs/harness/tapes/bitgetusdc-private.jsonl`, respectively.

Public coverage has 39 records: one metadata record, two REST reads, and 36
WebSocket frames across nine declared ticker/depth stages for each symbol.
Each book has a snapshot, absolute size updates, removals and reinsertions.
Run public replay with `--books BTCUSDC,ETHUSDC`.

Private coverage has 33 records, including 11 WebSocket frames and five commands:
place a caller-ID limit order, amend it, cancel it, query bounded historical
fills, and query account identity. It includes both symbols' positions/settings,
USDC collateral, normal/algo pending reads, and explicit Classic-account
detection. Three required mutations declare their exact native payloads. The
existing TS batch `marginMode: "cross"` spelling is separately declared as
`tsRequestBody`; the native payload keeps the documented `"crossed"`. The tape
matcher still checks native product, collateral, symbol, amount, price and
caller ID. Run private replay with `--books ''`.

## Observed correctness results

Local checks on 2026-10-20 used the already-built combined Node WASM artifact
(`11,948,833` bytes; SHA-256
`b440a9e996bc45e3f8b4428742e4ffa70e8da8a991d9f2b9b663ae40bdacc039`).
These are correctness observations, with no performance claim.

- Ten fixture tests pass, including independent zlib CRC32 reconstruction and
  negative request matches for wrong/missing product, wrong symbol and wrong
  collateral.
- Public TS and Rust replay each play 36 frames with two markets/tickers/books
  and five fixed checkpoints. Markets, tickers, books, requests, subscriptions,
  WebSocket requests and checkpoints match exactly. Both implementations have
  zero unserved requests, missing/unused mutations, invalid signatures or
  protocol errors.
- Canonical private TS plays 16 events (11 frames plus five commands), completes
  all five commands, and captures nine checkpoints with all transport audits
  clear. Canonical private Rust **fails** with
  `ReplayTimeout: command getAccount`. The Bitget venue does not handle
  `FetchAccount`; this must remain a known failure rather than dropping the
  TS-supported command or exempting general timeouts.

A temporary `/tmp` diagnostic tape omitted only the last `getAccount` command
to inspect financial output before that timeout. It is **not the canonical
private gate**, and it is not evidence that the canonical gate passes. Its
Rust replay completes 15 events/eight checkpoints with clear transport audits;
the first four command results match canonical TS results, including historical
fill ID `usdc-execution-1`, price `49000`, quantity `0.002`, commission `0.01`
`USDC`, and `positionSide: "both"`. Native private subscriptions match the TS
USDC account/order/algo/position subscriptions.

That bounded diagnostic also exposes these final financial divergences:

- Orders: TS drops `clientOid`, returning null client IDs; Rust retains
  `external-usdc` / `resting-eth` on the two final open orders.
- Live fills: TS reports null execution price; Rust reports `49000`.
- Balance, hedge mode, positions and settings match in this diagnostic,
  including realized PnL `2.89` / `1.89`. Live fill execution ID, fee and
  timestamp remain null on both outputs; matching nulls do not establish that
  those fields survive the live SDK/engine event path.

No fixture exclusions, output rewrites or venue implementation changes were
introduced to make these failures pass.
