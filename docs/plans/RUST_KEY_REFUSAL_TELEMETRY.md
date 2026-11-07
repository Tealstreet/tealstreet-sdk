# Key-refusal venue-code propagation

## Bitget finding and first PR

`BitgetFutures::refuse_route` and `private_route_exhausted` in
`sdk-rs/venues/bitget/src/account.rs` receive the actual 40018/40024 but emitted
text-only `auth.rejected` notices. The exhausted message contains a bare code;
NX intentionally extracts only labelled or parenthesized codes from text.
It can therefore classify `Invalid IP` as `ip_not_allowed` without a venue code.
The fix uses the existing `VenueError::venue_code` and `Cx::notice_error` path,
preserving message, severity, routing, retry and startup behavior.

Both production-driver assertions were RED before the fix. The existing worker
RPC path passes four preservation controls: actual `ExchangeError` through
`CallMethodHandler`, structured clone and `BaseExchange` rejection, with codes
40018, 406, 10003 and -2015. No worker serializer change is needed.

Read-only NX comparison: `commandOutcome.ts`, `CallMethodHandler.ts`,
`exchanges/base.ts`, Bitget `account.rs`/`trading.rs`, `keyRefusalTelemetry.ts`
and `exchangeController/exchangeAccountController.ts` have identical blobs at
`c7f74e52`, `99c547d4` and `6bf9ba14`. This is a path-specific producer gap,
not evidence that the intervening PRs generally stripped codes. The exact
Sentry event's route is not independently verified by this code-read.

## Second PR: evidenced numeric-code drops

Preserve a venue's actual numeric code as an exact string `venueCode` from
decoding through its `VenueError`, diagnostic, worker RPC and telemetry tags.
Never infer a code from prose, invent one for a local error, or use an HTTP
status as a vendor code. Keep free response bodies and credentials out of new
diagnostics. Existing outcomes, auth decisions, retries and user-facing text
stay intact.

| Producer | Current evidence | Second-PR change and control |
| --- | --- | --- |
| Shared `engine/src/engine.rs`, `Cx::startup_read_refused` | Accepts a typed error, then `notice(..., &error.message)` loses fields before `startup_failed(error.clone())` retains them. | Use `notice_error` at that call. Native RED must compare auth notice fields with the unchanged startup failure, preserving one-shot and secondary/loaded guards. |
| Binance `src/account.rs` | `read_error` already carries parsed numeric `code` via `classify::read`; startup uses the shared helper. | Shared fix; preserve -2015 and permission codes already present. No new parser. |
| Binance `src/options.rs` | The -2015 refusal extracts message and rebuilds a kind-only error. | Retain actual -2015 in that error; RED through option-read startup refusal. Local missing-credential errors remain code-less. |
| Bybit `src/lib.rs` | `reject_startup_key` emits a text-only duplicate notice, then constructs a typed startup failure with the actual code. | Build the typed error once and use it for both; RED on the notice and unchanged terminal behavior. |
| Bybit `src/wire.rs` / `src/account.rs` | `OpFrame` models success and ret_msg but retains no numeric code when one is supplied; the auth failure emits text. | Preserve supplied integer control-frame metadata on auth notices, with a production WS RED. The [official private-stream example](https://bybit-exchange.github.io/docs/v5/ws/connect#authentication) has no numeric code: retain that code-less behavior. Do not claim every auth frame supplies a code, or derive one from ret_msg. |
| BingX `src/account.rs` | `key_refusal` already attaches `evidence.code` before calling the shared startup helper. | Shared fix; preserve the actual code without changing key-refusal admission. |
| AsterDEX `src/account.rs` | `on_account_response` supplies a typed -2014/-2015 or numeric whitelist refusal to the shared helper. | Shared fix; no credential reads or synthetic code for malformed replies. |
| ApeX `src/trading.rs` and account reads | Read/write classification already retains numeric native code. Private WS control headers currently model no numeric code. | Shared helper preserves the typed REST path. A WS decoder extension requires a documented field or capture first; do not invent one from ret_msg. |
| WOO `src/account.rs` / `src/socket_codec.rs` | HTTP typed errors already contain numeric code. WS decoding and notice discard it. | Shared helper fixes startup HTTP notices. Held PR #2996 owns numeric-only WS diagnostics; do not duplicate it or its rollout dependency. |
| Hyperliquid/Spot local key/address mismatch | Locally derived mismatch has no venue response and no numeric venue code. | Remains code-less. Not a propagation failure. |
| Coinbase/Spot HTTP-only credential refusals | Some responses expose only HTTP 401/403/404 and local fixed text, with no parsed numeric vendor code. | Remains code-less. HTTP status belongs in status diagnostics, not a fabricated `venueCode`. |

The second PR carries no Bitget change or #2996 dependency. It tests the shared
notice path and the two evidenced producer omissions before editing runtime.
Run scoped engine/venue and worker checks, strict clippy, fmt and bans. Both
PRs remain unmerged for review and the next SDK cut.
