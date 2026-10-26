# Integer protocol wire stamps

Owner decision, 2026-10-22: fractional MEXC timestamps are a port artifact;
TypeScript uses integer `Date.now()` milliseconds. Receipts remain safe-integer.
This shared correction lands after #1723 and before RequestContext.

1. Add one engine `time::wire_stamp_ms(protocol_ms)` policy (floor, never round),
   exposed by `Cx::protocol_stamp_ms()` and `DispatchContext::stamp_ms()`.
   Calibration remains fractional and is applied before flooring. Raw `Cx.now`,
   deadlines, elapsed freshness, calibration revisions, durations and receipt
   validation remain unchanged. Invalid nonfinite inputs remain invalid, rather
   than becoming a valid zero stamp through an integer cast.
2. Audit every venue timestamp producer. Route numeric millisecond signing
   recipes through the shared helper, including the Binance reference. Correct
   MEXC REST/WS and WOO WS fractional timestamps and Backpack nearest-integer
   formatting. Preserve explicit seconds/ISO grammars, monotonic nonce domains,
   incoming timestamps and business query bounds. Subsequent signer migrations
   consume the same dispatch accessor; this does not move unmigrated signing to
   dispatch or claim it does.
3. Regress fractional host and calibrated clocks, signature/header/body stamp
   agreement, integer receipt compatibility and unchanged scheduling domains.
   Run focused engine/touched-venue/Binance tests, strict Clippy/fmt/bans, root
   checks, independent review, then full CI/timing/budgets and complete raw proof.
   Declare every intentional replay timestamp/signature difference with the
   integer-wire reason and TS control. No comparator, manifest, allowance or
   receipt-validation relaxation. Integrate actual #1723 before final proof;
   guard separately and merge only on green.

Architecture gate: this fixes a cross-cutting clock policy in one engine helper;
venue changes only select that policy at their existing timestamp boundary.
No provider/setup feature-guard changes and no NX API changes are planned.

## Re-verified timestamp inventory

| Existing boundary | Before this correction | Action |
| --- | --- | --- |
| MEXC REST Request-Time/preimage; WS reqTime/preimage | Fractional f64 formatting | Shared floor, same stamp in wire and canonical bytes |
| WOO private WS timestamp/preimage | Fractional f64 formatting | Shared floor, same stamp in wire and canonical bytes |
| Backpack REST preimage/header and WS preimage/argument | Nearest-integer `.0` formatting | Shared floor before formatting/signing |
| Binance, Bybit, Aster, BingX, Bitget, Bitunix, BloFin, WOO REST, WEEX, ApeX current-ms, OKX ISO input | Existing floor | Select shared helper; retain signatures at integer time |
| KuCoin, Orderly, Crypto.com | Existing integer cast | Shared floor before existing cast; preserve validation and wire type |
| Kraken | Validated integer milliseconds scaled to monotonic nonce | Shared floor inside existing validation; retain scale/domain/sequence |
| Phemex, Coinbase, ApeX legacy auth, OKX/Bitget WS | Explicit seconds/expiry grammar | Preserve grammar and conversion; no duration changes |
| Hyperliquid, TxFlow, GRVT, Paradex, Lighter, Polymarket, Bitrue | Typed integer nonce/stamp or separate expiry/session logic | No fractional wire correction identified; retain nonce/domain semantics |
| Lighter ping/pong, Bitunix ping | Unsigned heartbeat timestamp | Outside signing recipe correction; unchanged |
| MEXC stop-plan start/end and history query bounds | Business query ranges | Unchanged; not signing timestamps |

TS controls remain executable and unchanged: MEXC REST `mexc.api.ts` and WS
`mexc.ws-private.ts`, WOO `woo.ws-private.ts`, and Backpack
`backpack.signing.ts` use `virtualClock.getCurrentTimeMs()` for signing. Its
production source is `Date.now() + offsetMs`; an injected fractional tape clock
can still expose fractions in TS. A declared Rust integer correction does not
claim fractional-tape TS equality. CI archive receipts normalize certain auth
fields; native preimage/wire regressions separately establish the exact stamp.
