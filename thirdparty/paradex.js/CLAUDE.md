# CLAUDE.md — @tealstreet/paradex-sdk

Vendored JavaScript SDK for Paradex (StarkNet-based perpetuals DEX).

- **Tracked version**: 0.6.0
- **Dependencies**: `starknet`, `@starkware-industries/starkware-crypto-utils`, `bignumber.js`
- **Build**: `yarn build-original` runs the upstream `tsc`; the standard `build` is a no-op so the monorepo can use tsconfig-paths against `src/`
- **Used by**: `packages/safe-cex` Paradex adapter
- **Why vendored**: SDK is still pre-1.0; we hold it in-tree to apply our own fixes without rebasing on every upstream release
- **Patches**: see `git log -- thirdparty/paradex.js` for the diffs we carry
