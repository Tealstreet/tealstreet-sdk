# CLAUDE.md — @tealstreet/grvt-client

Tealstreet fork of `gravity-technologies/grvt-js-client` — Node/JS client for GRVT REST APIs and WebSockets.

- **Tracked version**: 1.6.8 (per `package.json`, marked as Tealstreet fork in the description)
- **Used by**: `packages/safe-cex` (the GRVT exchange adapter) and `thirdparty/grvt-ts-sdk`
- **Key patch**: order-payload field ordering fix for `builder` / `builder_fee` (the upstream serializer reordered them, breaking signing)
- **Why vendored**: forked specifically to carry the field-order fix without waiting on upstream releases
- **Patches**: see `git log -- thirdparty/grvt-js-client` for the diffs we carry
