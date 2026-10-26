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

- **D1, login signing failure: RULED (b), fe7lgm (engine shape).**
  - Engine fact: a legacy socket's `send_signed` failure takes
    `Engine::sign_failed`'s WsSend arm. The signing latch records a provider
    refusal, then the engine calls `cx.close(socket)` and
    `venue.on_close(socket, 1006)`, a synthetic close the venue can't tell from
    a real abnormal one. Inferring a signing failure from that close (or from a
    pending-signing flag) would misreport a transport close that lands inside
    the signing window. So no heuristic.
  - Seam: a new default-no-op `Venue::on_socket_sign_failed(&mut self, cx,
    socket, &VenueError)`. It's called in `sign_failed` for WsSend/WsConnect on
    the LEGACY path only, before the close and `on_close`. It's never called on
    the managed path, which keeps its feed-stop reasons. The latch, the close
    and `on_close(1006)` are otherwise unchanged.
  - The seam is a separate engine-only first commit of this PR (a default method
    plus one call site), reviewed by fe7lgm, with paired Bybit, Bitget and
    combined sizes at about +0.
  - Engine control: a legacy socket whose login signing is refused calls the hook
    once, then the close and `on_close(1006)`; a managed feed gets no hook.
  - BloFin implements the hook as native's handling: exactly one
    `STREAM_UNAVAILABLE`. The following `on_close(1006)` → `private_close` does
    the reconnect bookkeeping and adds no second notice.
  - This supersedes dazzy3's (a), which dazzy3 offered unless ruled otherwise.
- **The signing latch (shared engine rule; raised for 4yjlw0).**
  - A PROVIDER refusal (for example MissingKey) latches the private socket
    role. `SigningLatch.roles` is documented as "Socket roles whose signing was
    refused; they are never reconnected", so there is no reconnect and no
    retry, and no tight loop. The latch notice is "Signing cannot succeed with
    the installed credentials: …".
  - `credential_refused` attributes a refused socket login to the Primary
    (signing_dispatch.rs). The resulting credential notice text and count are
    pinned so the refusal is visible, not silent (fe7lgm).
  - An ENTROPY failure is not a provider refusal: the socket reconnects and the
    login signs again, as native retried.
  - 4yjlw0's pin "the next reconnect signs again after a provider refusal"
    conflicts with this shared rule. It is not worked around: the RED pins the
    latch as specified, and the rule stays the engine's.
- **D2, the sign-module ban (dazzy3: join).** BloFin joins `check-signing.py`'s
  migrated set: its `sign` directory is scanned, and `sign/dispatch.rs`'s HMAC
  work references are frozen as the migrated families' are. Leaving it exempt
  after the cutover would hide a regression.

## Original controls (RED first)

- The login frame is byte-equal to native for a fixed stamp, nonce and identity:
  key, passphrase, timestamp, sign and nonce, with field order and the JSON
  number timestamp.
- With a Secondary configured, the login signs with the PRIMARY identity, as
  native does.
- A login signing failure gives exactly one `STREAM_UNAVAILABLE` through the
  hook, closes the socket with no login frame, and adds no second notice on the
  `on_close(1006)` that follows (D1 (b)).
- A provider refusal latches the private role: no reconnect and no further
  login. The latch and credential notices are pinned by text and count, and the
  refusal is attributed to the Primary.
- An entropy failure reconnects and the next login signs again.
- Public-data-only with keys configured never calls `send_signed`: no Sign
  action and no login frame (4yjlw0).
- An explicit empty Secondary passphrase still refuses startup before discovery
  or entropy, with the same text, kind, notice and `StartupFailed`.
- `signed()` presence, the passphrase refusal and the "read-only API key" text
  follow identities; public-data-only still runs no private work.
- No raw key, secret or passphrase field remains on `Options` (a
  construction-shape control).

Existing tests keep their assertions. Fixtures that built raw options build
identities through the trusted setup (`tests/trusted/mod.rs`) instead. Today
that affects every BloFin test target that constructs `Options` directly
(about ten: support, trusted, hmac_dispatch_original, startup, startup_auth,
startup_degraded, security, trading, local_leverage_context, public,
managed_public and public_supervision). The only production constructor is the
registry.

## Gates

BloFin scoped tests, clippy `-D warnings`, fmt, `check-bans.sh`, the credential
and signing inventories at 0 for BloFin, and one local composed combined
measurement on the current tip (over +30K goes to the overseer). No feature
is added, so a shrink is expected (`sign.rs` and the raw fields go), and the
measured delta is stated. Then one frozen
CI run, READY and an exact-tree GO.
