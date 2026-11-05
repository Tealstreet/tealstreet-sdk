# FrameApplier settings leverage guard

**Change.** The SETTINGS section reads its total, buy and sell leverage through
`finite()`. A finite side still overrides the total. A symbol whose buy or sell
leverage is still non-finite is left out of `fetchedLeverage`, which means no
known setting, instead of being stored as NaN or 0.

**Why.** Bybit's settings copy the position leverage, which `float_of` maps to
NaN when it is absent. `UiStateHandler` then copied that NaN back onto
`position.leverage`, undoing the #2376 guard. Storing 0 would instead have
written leverage 0 onto the position.

**Callers.**
- SDK: `fetchedLeverage` holds only finite values. Order-form fallbacks see a
  missing symbol, as for any symbol with unknown rails.
- NX: none beyond picking up the safe-cex import; no wasm release.

**Obligation.** None.

**Deletion prerequisites.** None added.
