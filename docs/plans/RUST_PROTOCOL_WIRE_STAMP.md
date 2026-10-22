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
