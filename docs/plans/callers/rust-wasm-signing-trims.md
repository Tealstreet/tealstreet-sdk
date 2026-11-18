# Per-family hosted-signing WASM feature

Applies to browser/WASM and worker callers. Owner: claude-m26ps4. This is the
shared size trim that the Paradex local-signer change (#2330) owed.

## Exports

The WASM host's hosted-signing exports (`hostedSigningClock`,
`prepareHostedSigning`, `localHostedSigning`, `finishHostedSigning`,
`cancelHostedSigning`) now exist only in families built with the
`hosted-signing` feature. Those are Binance, Bybit, OKX, Bitget, Bitunix and
AsterDEX. Every venue whose construction rules enable `remoteSigning` is in one
of them, and `harness/construction-features.test.mjs` enforces that. The JS
host already fails closed when an export is absent, so no caller changes.

## Size

CI run 40610920529 shrank every non-hosted dispatch-signing family by about
48-49 KB: Paradex by 1,452,842 → 1,403,676 and Hyperliquid by
1,413,224 → 1,363,789. Their overrides are lowered to ceil KiB + 2 KiB:
Paradex 1373 KiB and Hyperliquid/Spot 1334 KiB. Hosted families moved by at most +9 bytes.
