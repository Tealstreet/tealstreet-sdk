# Shared dispatch-signing provider feature

Status: planned; extracted from Bybit #1728 at owner request.

The trusted registry and WASM provider bootstrap currently compiles only with
Binance. Expose one `dispatch-signing` feature shared by migrated families,
without enabling another family or changing credential, signature, or request
semantics. Binance enables the feature transitively. Future families opt in
in their separate migration PRs.

1. Move generic provider setup and external-provider construction gates from
   `binance` to `dispatch-signing` in the registry and WASM host. Keep family
   factories and construction declarations unchanged.
2. Add regressions proving trusted primary/secondary setup works without the
   Binance feature, clears raw input, and preserves passphrase/identity data;
   unmigrated families must still reject provider injection. Exercise the
   existing Binance reference and combined host fixtures.
3. Run focused native tests, strict Clippy and formatting; obtain independent
   correctness/adversarial reviews and fresh CI. Integrate actual #1726 before
   final proof/guard. Merge this seam before Bybit, then merge actual landing
   into #1728. Other family migrations start from the landed seam.

Architecture gate: this is the shared provider-construction choke point, not
a per-venue implementation. No new caller contract or venue adoption occurs.
