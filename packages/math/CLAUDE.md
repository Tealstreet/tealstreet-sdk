# CLAUDE.md — @tealstreet/math

Mathematical utilities for precision handling, rounding, distribution, and numeric calculations used across the trading platform.

- **Key files**: `getPrecisionFromSigDigits.ts`, `distributeValues.ts`, `handleRoundingError.ts`, `getSigDigitsFromPrecision.ts`, `convertDimToInt.ts`, `convertPxStrToInt.ts` (34 source files total)
- **Build**: `yarn build-force` (tsup) — normal build disabled for tsconfig path aliases
- **Dependencies**: dequal
- **Used for**: Price/quantity precision handling throughout the platform
