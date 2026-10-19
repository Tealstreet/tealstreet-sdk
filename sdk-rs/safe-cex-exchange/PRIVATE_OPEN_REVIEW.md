# Private socket open review

This closes assignment rows 9 (Bybit), 11 (Paradex) and 68 (Orderly).
The maintained TypeScript bodies remain the authority. The bounded differences
below remain `divergent`; passing native recovery tests is not source equivalence.

| Row | Maintained source | Native disposition |
| --- | --- | --- |
| 9 | Bybit `onOpen` calls `sendReconnectSequence` and returns undefined without awaiting clock sync or authentication. Its detached promise survives disposal. | ACCEPT: the callback still returns immediately, starts clock sync synchronously and preserves active auth/subscription order. The existing core disposal contract cancels pending tails and releases the weak venue; post-disposal work is intentionally suppressed. No live result or diagnostic is omitted by this acceptance. |
| 11 | Paradex `onOpen` resets auth state, logs, pings, and awaits `authenticate`. The latter catches token/signing rejection, logs the original error and resolves, leaving the socket unauthenticated. | ACCEPT: constructed onOpen additionally disregards silence and reconnects after that caught failure. Direct authenticate retains the source catch/log/resolve. This is visible recovery from an otherwise idle unauthenticated socket, not a source-equivalence claim. |
| 68 | Orderly `onOpen` signs synchronously before sending auth; a signing exception escapes the event listener, with no auth/subscription frame and no explicit recovery. | ACCEPT: the constructed callback reports the original error, resolves and reconnects through the same shared recovery. The changed listener outcome/reconnect is explicit; successful auth and server-ack subscription ordering are unchanged. |

## Diagnostic correction

`BaseWebSocket::authenticate_and_subscribe` discarded an `Err` after checking
`is_err`, retaining only a generic reconnect warning. That lost the Orderly
signing error; Paradex had a source text log but the shared reporter never received
the cause. The direct cutover reports the supplied `ExchangeError` through the
existing runtime reporter, then keeps the warning/reconnect sequence. It neither
constructs a replacement error nor adds a subscription or host panic.

The actual Orderly constructed-socket regression fails on the unchanged shared
body: expected one `Non-base58 character` report, observed none. The Paradex constructed
exchange similarly requires one report retaining the source catch's cause. The
core regression also checks original thrown-object identity, name and message;
success/disposal controls retain their existing outcomes. These are diagnostic
FIXes, separate from the bounded ACCEPTs above.

The helper also recovers an error returned by the original open prefix (for
example, a log or ping failure). This existing callback outcome difference is
included in the recovery ACCEPT; the correction reports that cause too.

A production caller census finds only Orderly, Paradex and BloFin using this core
helper. BloFin therefore inherits the cause report too; its existing login and
private-frame regressions must pass. Bybit owns its separate reconnect operation
and does not use this helper.

## Reproduction

Run from `sdk-rs`, with frozen Node dependencies and `SAFE_CEX_TYPESCRIPT_PATH`
pointing at the workspace TypeScript package. Use `CARGO_INCREMENTAL=0`, jobs 1.

- Orderly: lib `private_signing_failure_sends_nothing_and_uses_shared_recovery`;
  `--test ws_controls private_open_native` executes the maintained source body's
  success/disposed/sign-throw/snapshot-throw traces. The encryption source oracle
  independently confirms Error `Non-base58 character` for the constructor-normalized invalid secret.
- Paradex: `--test exchange_trait private_signing_failure_sends_no_auth_or_subscribe_and_recovers`;
  `--test private_auth` includes the maintained onOpen/authenticate cases and
  preserves original catch/log error identities and partial-effect ordering.
- Bybit: lib `concrete_private_protocol_composes_source_open_sequence_and_real_auth`
  with `--include-ignored` constructs the production exchange/socket and compares
  eight actual-source local/remote/disposed profiles, immediate return, frames,
  signer envelopes and errors. Lib
  `reconnect_sequence_disposal_cancels_unpolled_pending_and_inline_pending_tail`
  checks the intentionally different disposal boundary.
- Core: lib `auth_failure_sends_nothing_disregards_silence_and_never_auth_invalid_phrase`
  and `auth_await_disposal_blocks_subscribe`.

No browser/Wasm-only row is closed by these native observations.
