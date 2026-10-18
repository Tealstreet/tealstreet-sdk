Bitrue keyless public capture from the live-diff lane, 2026-10-18.

- `bitrue-contracts.json`: exact `GET https://fapi.bitrue.com/fapi/v1/contracts` 200 body, SHA256 `e10d408a6ed312733ea0170eeb11f7935cbd661191b581f7a2aabbad4de29a72` (779 markets).
- `bitrue-gzip-prefix.ws.json`: first entry, unchanged bytes/binary flags/timing, of tape `b49c49c7b98c0bd2b555b8adb0a535671ed106ec0771c2a2b1365d9dcdf87c76.ws.json` (126 frames). The BTC gzip ticker frame is sufficient to reproduce native zero prices and `Unknown data [object]`.
- Prefix file SHA256: `e38b3701084b2a7d1b64e8ef3b1d6f57701848a4713f6971a5871154f7bf1c65`.

`node --test sdk-rs/safe-cex-ffi/tests/native-bitrue-gzip.test.mjs` uses the built production addon, real native/TS transports and venue handlers. Child-only TLS redirection serves the same recorded HTTP body and one binary WS frame, with zero upstream forwarding; complete 779-row markets/tickers are compared. Set `LIVE_DIFF_ADDON` to select an immutable baseline. This focused single-frame test establishes binary projection and value parity, not drain/timing parity for the full 126-frame tape.
