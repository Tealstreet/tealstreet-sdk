# CLAUDE.md — @tealstreet/grvt-ts-sdk

Tealstreet fork of the higher-level GRVT TypeScript SDK. Wraps `@tealstreet/grvt-client` and adds account management, transfers, and a typed signature layer.

- **Layout**: `src/` exports `GrvtClient` (REST) and `GrvtWsClient` (WebSocket)
- **Depends on**: `@tealstreet/grvt-client`, `ws`, `ethers`, `@metamask/eth-sig-util`
- **Used by**: `packages/safe-cex` for GRVT account / trading operations
- **Why vendored**: locked alongside the `grvt-js-client` fork so the field-order patch and signing path stay consistent across both layers
- **Patches**: see `git log -- thirdparty/grvt-ts-sdk` for the diffs we carry
