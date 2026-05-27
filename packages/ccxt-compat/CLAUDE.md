# CLAUDE.md — @tealstreet/ccxt-compat

CCXT compatibility layer providing number precision utilities and type helpers ported from the CCXT library.

- **Key files**: `Precise.ts` (arbitrary precision), `functions/number.ts` (precision helpers), `functions/ccxt_exchange.ts` (exchange utilities), `types.ts`
- **Build**: `yarn build-force` (tsup)
- **Exports**: `ROUND`, `TRUNCATE`, `DECIMAL_PLACES`, `precisionFromString`, `decimalToPrecision`, `truncate`
- **Note**: Linting disabled (third-party compatibility package)
