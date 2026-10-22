# Shared dispatch-signing provider feature

Status: implemented locally; extracted from Bybit #1728 at owner request. CI and actual #1726 integration pending.

The trusted registry and WASM provider bootstrap currently compiles only with
Binance. Expose one `dispatch-signing` feature shared by migrated families,
without enabling another family or changing credential, signature, or request
semantics. Binance enables the feature transitively. This is an additive feature: the
existing requirement to select at least one venue remains in force. Future families opt in
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

Focused validation: the new standalone registry test target fails to compile
with the old Binance-only gates (both trusted setup APIs are unavailable),
then passes all five groups with the shared gates. A no-default-feature
Bybit+dispatch-signing build passes 17 host tests, including marker refusal
and unchanged public startup; the combined host passes 19 tests. Existing
Binance native tests and registry unit tests pass. Standalone and combined
strict Clippy, formatting and banned-pattern checks pass. Independent
correctness/adversarial review is clean on the four production files.

These tests deliberately pin Bybit as an unmigrated negative control at this
commit; its later migration must update the control with its positive tests.
There is no new SDK/NX API or persisted identity change in this extraction.
