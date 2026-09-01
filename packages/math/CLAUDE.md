# CLAUDE.md — @tealstreet/math

Mathematical utilities for precision handling, rounding, distribution, and numeric calculations used across the trading platform.

- **Key files**: `getPrecisionFromSigDigits.ts`, `distributeValues.ts`, `handleRoundingError.ts`, `getSigDigitsFromPrecision.ts`, `convertDimToInt.ts`, `convertPxStrToInt.ts` (34 source files total)
- **Build**: `yarn build-force` (tsup) — normal build disabled for tsconfig path aliases
- **Dependencies**: dequal
- **Used for**: Price/quantity precision handling throughout the platform

## `roundToNearestMultipleDown` treats a near-integer quotient as exact

`Math.floor(n / m)` is wrong for values people type. `0.3 / 0.1` is
`2.9999999999999996`, so a plain floor gave `2` and
`roundToNearestMultipleDown(0.3, 0.1)` returned **0.2** — a third of the order
gone, on a value that is an exact multiple of the step. `0.7`, `2.9`, `8.2`
and `0.29 / 0.01` all did the same.

This is the round-DOWN branch of `script-api-core`'s `roundOrderSize`, which
Hyperliquid SPOT placement uses so a buy cannot exceed the balance held.
Under-sizing by a whole lot step is the opposite of the safe failure it exists
to produce, and it reached the venue.

The function now snaps a quotient that is an integer within a relative epsilon
to that integer before flooring. **Do not simplify it back.** The old test
suite passed because every case it checked (`0.5/0.1`, `1.5/0.5`, `2.0/0.5`)
divides exactly in binary; the `it.each` blocks added beside it are the ones
that fail against a bare floor. `roundToNearestMultiple` never had the bug —
`Math.round` absorbs the noise.
