# CLAUDE.md — @tealstreet/utils

General-purpose utility functions for compression, formatting, account parsing, and API policy management.

- **Key files** (23 total):
  - `formatting/` — `formatPrice.ts`, `formatCurrencyValue.ts`, `formatCompact.ts`, `formatLargeAmounts.ts`
  - `accounts/` — `parseImportAccountData.ts`
  - `compression.ts`, `base64ToBlob.ts`, `stableHash.ts`, `sleep.ts`
  - `getApiPolicy.ts`, `getApiPolicyUrl.ts` — API endpoint configuration
- **Build**: `yarn build-force` (tsup)
- **Has barrel export** from `index.ts` (intentional — API surface)
- **Dependencies**: `@tealstreet/sleep`
