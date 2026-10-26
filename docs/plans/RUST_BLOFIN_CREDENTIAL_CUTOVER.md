# BloFin credential cutover: WebSocket login and raw options

Base: `feat/rust-rewrite` `25ca8466d63`. Owner: `claude-kuhwxi` (overseer
assignment, 2026-10-26). Reviewers: `claude-dazzy3` or `claude-fe7lgm` (source),
`claude-4yjlw0`. Landing: `claude-8x5tm1` exact-tree GO.

After #2143 and #2157 (W1), every BloFin REST call signs through the #2084
dispatch recipe over opaque identities. Raw material survives in three places:

- the private WebSocket login, which signs with `sign::signature` over the raw
  option and sends the raw key and passphrase in the login frame;
- `Options.key`, `Options.secret`, `Options.passphrase` and
  `Options.read_credentials`, used for presence checks, the login and a startup
  validation;
- the registry, which copies the raw strings into `BlofinOptions` beside the
  identities it already built.

This slice moves the login onto the dispatch provider and deletes the raw
fields. It clears BloFin's credential-selection inventory (5 sites) and signing
inventory (11 sites). BloFin sockets themselves (the legacy socket family, 4
sites) are out of scope and wait for claude-55ao7t's per-venue feature-runtime
investigation.

## Shape

- **WebSocket login: a `WsLogin` dispatch recipe** in `sign/dispatch.rs`,
  modelled on OKX's `WsLogin`. `private_open` calls
  `cx.send_signed(socket, String::new(), WsLogin::new(primary, url))`.
  - `prepare` attests the outbound is a `WsSend` and takes the dispatch stamp.
    It refuses an unusable identity before drawing entropy (`ready`), then draws
    the 21-byte nanoid nonce from signing entropy, as the REST recipe does.
  - It builds the same HMAC-SHA256 work as today's login. The preimage parts are
    `/users/self/verify`, `GET`, the millisecond stamp, the nonce and an empty
    body. The context's host is the private socket's host, plus `Present`
    passphrase.
  - `apply` writes the exact native frame from the provider's values:
    `{"op":"login","args":[{"apiKey","passphrase","timestamp","sign","nonce"}]}`.
    `timestamp` stays a JSON number, and `sign` is base64 of the MAC's lowercase
    hex.
  - The `ping` that follows the login, the open flag and the supervisor calls are
    unchanged.
- **Presence from identities.**
  - `signed()` becomes `!public_data_only && identities.primary.is_some()`.
  - The `private_start` passphrase check reads the primary identity's
    passphrase ref.
  - The startup-rejection text that chooses "read-only API key" reads
    `identities.secondary.is_some()`.
- **Startup validation.** The registry's trusted setup (`blofin_signer`) already
  applies native passphrase inheritance and drops an empty effective
  passphrase. The registry's shared `validate_read_key` already refuses an
  invalid read key at construction. BloFin's `start()` keeps native precedence
  for the explicit-empty Secondary passphrase case from identities alone: a
  Secondary identity with no passphrase ref refuses startup with the same text,
  kind (`InvalidInput`), `CONFIG_INVALID` notice and `StartupFailed`, before
  discovery or entropy.
- **Options.** `key`, `secret`, `passphrase` and `read_credentials` are deleted,
  along with their `EraseSecrets` arms. BloFin keeps `testnet`,
  `broker_attribution`, `public_data_only` and `identities`. The registry stops
  copying the raw strings; `blofin_signer` keeps reading them from the trusted
  engine options, as setup does for every family.
- **Orphans.** `sign.rs` (`sign::signature`) and `mod sign` go once the login is
  their last caller.

## Decisions for review

- **D1, login signing failure.** Today a nonce-entropy failure in `private_open`
  emits `STREAM_UNAVAILABLE`, closes the socket and runs `private_close`, so the
  supervisor reconnects. Under `send_signed`, a provider refusal or an entropy
  failure surfaces through the engine's staged-signing failure path. The RED pins
  native's visible behaviour: one `STREAM_UNAVAILABLE`, the socket closed,
  reconnect scheduled, no login frame sent. Source routes the engine's failure to
  the same native handling. If the engine's socket-signing failure cannot reach
  the venue, that's raised before source rather than worked around.
- **D2, the sign-module ban.** `check-signing.py` exempts unmigrated families'
  `sign` modules. Whether BloFin joins the migrated set (its `sign` directory
  scanned, with frozen test-oracle sites) is a reviewer call. This plan proposes
  joining, with `sign/dispatch.rs`'s HMAC work references frozen as the
  migrated families' are.

## Original controls (RED first)

- The login frame is byte-equal to native for a fixed stamp, nonce and identity:
  key, passphrase, timestamp, sign and nonce, with field order and the JSON
  number timestamp.
- With a Secondary configured, the login signs with the PRIMARY identity, as
  native does.
- A login signing refusal (provider or entropy) gives exactly one
  `STREAM_UNAVAILABLE`, the socket closed, a reconnect and no frame (D1).
- An explicit empty Secondary passphrase still refuses startup before discovery
  or entropy, with the same text, kind, notice and `StartupFailed`.
- `signed()` presence, the passphrase refusal and the "read-only API key" text
  follow identities; public-data-only still runs no private work.
- No raw key, secret or passphrase field remains on `Options` (a
  construction-shape control).

Existing tests keep their assertions. Fixtures that built raw options build
identities through the trusted setup (`tests/trusted/mod.rs`) instead. Today
that affects `tests/support/mod.rs`, `tests/trusted/mod.rs`,
`tests/hmac_dispatch_original.rs` and `tests/startup_auth.rs`. The only
production constructor is the registry.

## Gates

BloFin scoped tests, clippy `-D warnings`, fmt, `check-bans.sh`, the credential
and signing inventories at 0 for BloFin, and one local composed combined
measurement on the current tip (over +30K goes to the overseer). Then one frozen
CI run, READY and an exact-tree GO.
