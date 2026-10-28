### Health text patterns match the engine's code classification (beta classification R5)

Owner: claude-dazzy3. Plan: [RUST_ERROR_CLASSIFICATION.md](../RUST_ERROR_CLASSIFICATION.md),
ruling R5. This changes only `packages/safe-cex/src/health/errorPatterns.ts`, the
text fallback `ExchangeHealthMonitor` uses when an error carries no specific kind.

Category changes callers will see (the web reload pause keys on the `auth_*`
categories):
- **Binance -1022** moves from `time_sync` to `auth_invalid` (invalid
  signature; the engine reports it `authInvalid` after one re-signed resend).
- **OKX:**
  - 50112 moves from `auth_invalid` to `time_sync` (invalid timestamp), so a
    clock skew no longer pauses reloads.
  - 50101, 50103-50105, 50114 and 50119 are now `auth_invalid`.
  - 50100, 50110 and 50120 are now `auth_permissions`.
  - Codes match only as the engine words them (`OKX <code>:`) or after a `code`
    label, quoted or not (a raw body's `"code":"50111"`), so a price such as
    50111.5 no longer reads as a bad key.
- **429/502/503** match only as a bare status number. A client order ID,
  price or amount containing those digits (for example 4294967295) no longer
  reads as a rate limit or an outage.

Known TS/Rust policy difference, kept on purpose (claude-ddi2il ruling): Bybit
10005 (a permission refusal on one endpoint) is `auth_permissions` in TS, which
pauses web reloads, while Rust startup does not treat it as a credential
rejection (`sdk-rs/venues/bybit/src/lib.rs` `CREDENTIAL_REJECTIONS`). Rust is the
intended behaviour. A later cutover of the reload pause to typed kinds must not
reintroduce a pause on 10005.

Deletion prerequisites: none added or cleared.
