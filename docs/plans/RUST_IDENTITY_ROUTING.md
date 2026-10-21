# Rust SDK account identity routing

## Plan and audit

The SDK must expose the native identity continuations wherever the shipping TS
account API supports identity. A native-command read creates one host without
starting public streams or waiting for unrelated account reconciliation. Native
code owns endpoint selection, signer choice, session acquisition, exact decimal
IDs, primary-key fingerprinting and response validation. The SDK forwards the
existing command and returns its result without interpreting REST data.

| SDK aliases | Identity source and SDK dispatch |
| --- | --- |
| apexomni, kucoinspot | Existing `getAccount` command aliases; retain their constructor overrides |
| asterdex, backpack, binance, binancepm, bingx, bitget, bitgetinverse, bitgetusdc, bybit, bybitinverse, bybitspot, bybitdemo, cryptocom, grvt, kucoin, lighter, lighter-robinhood, mexc, paradex, phemex, polymarket, weex, woo | Initialize the host, then issue native `fetchAccount` |
| coinbase, coinbasespot | Preserve the existing ready native portfolio ID through `host.accountId()` |
| tealstreet-dex, kodiakfi, mode, salsadex, what, woofi | Preserve the ready configured Orderly application ID and affiliate ID |
| binancespot, binanceinverse, bitrue, blofin, breakoutprop, hyrotrader, hyperliquid, hyperliquidspot, kraken, krakenspot, okx, okxeu, okxdemo, okxeudemo, txflow | Explicit unsupported identity rejection before host initialization |

This accounts for all 48 registered Rust aliases: 33 supported and 15
unsupported. The inventory follows current TS `getAccount` overrides and their
product subclasses, plus the preserved Coinbase/Orderly adapters. In particular,
Binance Spot and COIN-M use independent Base subclasses, Bitrue and Breakout have
no TS identity override, and Hyro explicitly rejects identity. Kraken Spot's
native `FetchAccount` returns balances; it must never be exposed as identity.

The native batches and their per-venue tests are recorded in
[RUST_BRIDGE_FIXES.md](RUST_BRIDGE_FIXES.md), Phase 4d. Lighter's lookup is
unsigned and supports public-only use without a signer. MEXC and WEEX validate
their selected read credential before returning a primary-key-derived native
identity. These distinctions stay in native code.

Verify the complete alias inventory at the actual EngineHost command boundary,
large string IDs, optional/null fields and native failures. Cold reads must not
start streams or use seeded JS account state. Concurrent reads share one host;
load failure, disposal and terminal traps must not fabricate identity or retry.
Preserve the existing ApeX/KuCoin Spot, Aster, Coinbase and Orderly query tests.
Run focused SDK tests, package types and strict scoped lint, then independently
review the stable source. These fake-ABI controls do not replace a later combined
WASM identity acceptance run.

## SDK verification receipt

The wrapper routes the remaining 23 native-command aliases after host
initialization and preserves their terminal-engine error on later reads. The two
existing native command aliases, two Coinbase adapters and six Orderly brokers
retain their existing paths. All 15 unsupported aliases reject before host
construction; Kraken Spot never dispatches its balance command as identity.

The new 61-case suite covers the complete 48-alias inventory, cold queries with
unloaded JS state, exact large IDs, nullable/absent fields, unsigned Lighter index
zero, one shared host for concurrent reads, correlated native rejection, frozen
failure identity, initialization failure, disposal and terminal traps without
automatic retry. Fake ABI actions run through the real EngineHost; they do not
emulate per-venue HTTP or prove a rebuilt native artifact.

Final focused checks pass 128 tests across six files, including the unchanged
ApeX, Aster, KuCoin Spot, Coinbase read-options and parity-query suites. The JSON
receipt is `/tmp/bridge-identity-sdk-final.json`; logs are
`/tmp/bridge-identity-sdk-final.log`, `/tmp/bridge-identity-sdk-types.log` and
`/tmp/bridge-identity-sdk-lint.log`. SDK typecheck, strict scoped lint with ignored
tests explicitly included, and whitespace checks pass. Independent source review
and the later combined-WASM identity gate remain separate acceptance steps.
