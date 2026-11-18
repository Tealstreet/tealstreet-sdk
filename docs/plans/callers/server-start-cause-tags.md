# Hosted "account start failed" Sentry events carry closed cause tags

**Change.** The native exchange server's start-failure report gains two tags,
both added to the `before_send` allowlist (`sdk-rs/hosts/server/src/telemetry.rs`
`TAGS`). The rebuild model is unchanged: fixed summaries, no message text, no
account or user ids, no key material.

- `startup_unloaded`: the first of `markets`, `tickers`, `balance`, `orders`,
  `positions` not yet loaded when the start failed. The order and requirements
  follow the engine's `watch_startup`: tickers only with a public feed, and
  account sections only for an account startup. It says what had not loaded,
  never which read failed.
- `startup_refused`: `true` when the venue refused a bootstrap read over the
  key (`Notice::StartupFailed.read_refusal`), `false` otherwise.

The native server computes both where it handles `Notice::StartupFailed`, from
the engine it already holds. `ErrorReport` carries them to the sink. The engine
gains only a native-only `account_startup()` getter, so wasm is unchanged.

**Callers.** `ErrorReport` has two new fields; transport reports set them to
`None`.

**Obligation.** None. **Deletion prerequisites.** None added. Once the engine
records per-section startup state (the STARTUP_INCOMPLETE tracker), the server
can tag the true failing section and HTTP status from it instead.
