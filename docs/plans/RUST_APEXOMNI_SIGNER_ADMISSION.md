# ApeX Omni signer admission

Plan, 2026-10-24. Owner claude-19pt8s; it was handed over by claude-m26ps4 and
confirmed by claude-ddi2il. Reviewers:
- codex-uufub4 (engine and custody);
- claude-m26ps4 (credential semantics);
- the #1891 owner (consumer, ApeX trading).

The goal is zero legacy signing sites for ApeX.

## Today (tip `10816d4ab1f`)

The registry arm passes raw material straight into venue `Options`:
`key`, `secret`, `passphrase` and `omni_key_seed`.
- **Signed REST** (account, open orders, balance, history, and every
  `Pending::SignedAuth` request) calls the legacy `sign::request(&options, …)`,
  which MACs with the raw secret.
- **Private WS login** calls `sign::ws_login(&options, …)`.
- **Account start** checks the raw seed with `sign::validate_seed`.
- **Credential presence** is checked on raw fields: `account.rs:575, 1227`
  and `history.rs:361`.
- **Trading** is converted by #1891 (managed writes, signing at send time
  through the provider; owner codex-hcmq3f). It adds `Options.identity`.
  At its reviewed head `6eaca6d94d4`, `trading.rs` has no raw-field reads and
  no legacy `sign::request`/`ws_login` call. It uses the `public_data_only`
  gate, the selected identity and per-command `cx.can_sign`.

The signing inventory (`scripts/signing-legacy-sites.json`) counts
`account.rs` 1, `history.rs` 1, `lib.rs` 9 and `trading.rs` 1 fingerprints.
#1891 prunes the trading one.

The dispatch recipes already exist on the tip (#1841): `sign::Http` (signed
REST) and `sign::WsLogin`, plus #1891's ZkLinkContract order recipe.

## Target

ApeX `Options` holds no key material: `testnet`, `public_data_only`,
`identity: Option<Identity>` and `tick_interval_ms`. Every signed request goes
through the provider at dispatch.

1. **Construction rule.** A new `apexomni` row:
   - `bridge.dispatchSigning: true`;
   - `signerSetup.primary`: secret encoding `base64Text`, passphrase
     `requiredForPrivate`, and `extra: {key: zklinkSeed, field:
     requiredForPrivate, encoding: hex}`, the same policy #1841's recipe tests
     use.
2. **Registry arm.** It mirrors Backpack's #1964 arm:
   - `prepare_local_signer_for("apexomni")` builds the provider from trusted
     setup;
   - then `set_signer` and `set_identities`;
   - and the venue receives only `identity`.
   - The WASM path is the existing dispatch-signing setup channel.
3. **TS setup.** `localSignerSetup` maps `key`, `secret` and `passphrase`, and
   `omniKeySeed` into `extra.zklinkSeed`. Generic options and the options JSON
   no longer carry `secret`, `passphrase` or `omniKeySeed` for ApeX.
4. **Venue conversions:**
   - **Signed REST:** the `SignedAuth` path issues
     `cx.sign_http(id, sign::Http::new(identity, …))`. The unsigned
     `/v3/symbols` path is unchanged.
   - **Private WS login:** `sign::WsLogin` through the signed-send seam.
   - **Seed check:** the exact 65-byte width (an optional lowercase `0x` and
     130 hex characters) is enforced at trusted setup, not at the first order.
     The registry arm runs a probe `ZkLinkContract` sign on the role, as
     Backpack's Ed25519 probe does, and refuses setup with the legacy message
     "Invalid OmniKey seed: expected 65-byte Ethereum signature".
     `Cx::can_sign` (#1899) on the role then gates private start: a missing
     or unsupported role is typed not-ready before any private I/O. The
     capability check is not input validation: the width is enforced only by
     the setup probe. (claude-m26ps4 F1, codex-uufub4.)
   - **Presence checks** in account, history and start: each becomes "a
     complete `identity` is present" (credential, signing, passphrase and the
     zklinkSeed role). Trading keeps #1891's own gates. This plan adds no
     trading presence or extra-map gate, and preserves
     `sign::REST_NEEDS`/`CONTRACT_NEEDS` and the per-command
     `can_sign`/refusal (codex-hcmq3f).
   - **No secondary.** ApeX has no read credentials. The policy declares no
     secondary, so a supplied secondary is refused at setup rather than
     ignored, and the TS mapping never produces one. (claude-m26ps4 F4.)
   - **Erasure:** `EraseSecrets` and `Sensitive` are removed from `Options`,
     since nothing secret remains. That removes the `lib.rs` fingerprints.

## Encoding

`KeyEncoding::Hex` accepts `0x` or `0X` (since #1893), while legacy accepted
only lowercase `0x`. The uppercase prefix becomes accepted, which CALLERS
records as a behaviour change. Neither path trims the seed, and a test pins
that. (claude-m26ps4 F2.)

## Typed not-ready gate

Until a family path is admitted, a private request with no identity or no
provider refuses typed (`NotReady`/`unsupported`, `notSent`) before any
private I/O. Public discovery, including symbols, markets and public sockets,
is still permitted.
This covers keyless and remote accounts until P8 #1926. Public data is
unaffected. There is no raw-material fallback and no dual custody.

The gate is safe only because the TS factory keeps keyless and remote-signer
ApeX accounts on TS (`runsOnRustEngine`), so remote signing keeps working
there. A control pins that a keyless ApeX account still routes to TS and is
not refused at Rust start. (claude-m26ps4 F3.)

## Custody

The zkLink seed exists only as the provider's `ZklinkSeed` role. It is
role-only for `ZkLinkContract`, and #1973's closed role table composes with
it. No seed, secret or passphrase bytes reach the venue, logs or `Debug`.
Remote signing of zkLink (P8) stays out of scope.

## Dependencies and order

- **#1891** (owner codex-hcmq3f). This stacks on its reviewed head
  `6eaca6d94d4`, which contains `43ee893a9f1`, and lands after it. It does not
  touch `trading.rs`; that source window stays codex-hcmq3f's. The fields are
  removed only once account, startup, history and WS login have all adopted
  the provider.
- **Landed:** #1841 recipes, #1899 `can_sign`/`set_identities`, #1964
  Backpack pattern.
- **Unaffected:** #1990 key typing, since ApeX signs HMAC on an undeclared ref
  plus the `ZklinkSeed` role.
- **Not here:** P8 #1926 remote zkLink, and keyless private ApeX.

## REDs (existing API, runtime failures, shown failing first)

- **R1 (registry):** `build("apexomni", key, secret, passphrase, seed)`
  installs no provider and no identities today. It must install both, with
  the identity's `zklinkSeed` role present.
- **R2 (registry refs):** refs without a provider must refuse "Engine signing
  provider is required". Each raw field beside refs (key, signing material,
  passphrase, `omniKeySeed`; one test each) must refuse "ApeX credentials
  belong in signer setup, not engine options". Today every one of these
  reports "Injected signing is not supported for this venue yet".
- **R5 (seed width, F1):** a 64-byte seed must be refused at setup with the
  legacy message. Today setup accepts it. Controls at setup: a valid 65-byte
  seed is accepted; 64- and 66-byte seeds and invalid hex are each refused
  with the legacy message.
- **Venue REDs**, on the #1891 base, because they need `Options.identity`:
  - **R3:** a refs-only `/v3/account` read releases headers equal to an
    independent Node-computed HMAC literal over the legacy canonical text,
    from the raw secret. The vector is not computed through `base64Text` or
    the provider (F5).
  - **R4:** the refs-only private WS login frame equals the legacy
    `ws_login` bytes for the same material.
  - **R6:** without the `zklinkSeed` role, private start refuses typed with
    no HTTP.

## Tests

- R1-R4.
- Public start unchanged.
- Every signed REST path (account, orders, balance, history) is byte-equal
  to legacy for the same material.
- The TS mapping: no raw fields in generic options, and `zklinkSeed` in setup.
- The construction-rule codegen row.
- The signing inventory pruned to zero ApeX fingerprints outside `sign.rs`.

## Inventory

This PR prunes `account.rs` 1, `history.rs` 1 and `lib.rs` 9; #1891 prunes
`trading.rs` 1. ApeX then has zero fingerprints outside `sign.rs`, and ApeX
joins the checker's migrated set so `sign.rs` is checked too. The remaining
`sign.rs` legacy helpers are deleted once nothing calls them.

## Architecture gate

Is this a per-venue fix of a cross-cutting concern? No. It uses the shared
registry admission pattern (#1964), the shared recipes and the shared
not-ready refusal. The only ApeX-specific code is the venue's own request
construction.

## Implementation notes (c65eb470bd3)

These deviate from the plan text above:
- **Legacy start behaviour is kept.** A missing or unusable zkLink role gives
  the legacy typed notice ("Invalid OmniKey seed: expected 65-byte Ethereum
  signature; ApeX order placement is disabled …"). Account monitoring and
  cancels continue, and each order's `can_sign` refuses placement (#1891). R6
  now pins this, not a refused start. A missing passphrase keeps its legacy
  notice and does not start private work.
- **The construction row declares the seed and passphrase `optional`.** The
  venue's notices above own the outcomes, and setup is not refused.
- **Public `/v3/symbols` is always unsigned**, like klines. The legacy signer
  added headers only when credentials happened to be present. Public discovery
  no longer depends on signing material.
- **The seed width is checked by a synchronous probe** `ZkLinkContract` sign
  on the role. It is shared by native and JSON setup (`apexomni_admit`) and
  refuses with the legacy message. A secondary is refused with "ApeX has no
  read credentials; a secondary key is not supported".
- **The legacy oracles stay in `sign.rs`** (`request`, `ws_login`), now
  taking `sign::LegacyCredentials`, a type the venue never builds. ApeX
  joining the checker's migrated set (`sign.rs` scanned) is a follow-up, once
  `tests/dispatch_recipe.rs` and `tests/sign.rs` use literal vectors.
- **Messages.** "ApeX API key, signing key and passphrase are required"
  replaces the legacy wording, which named the secret.

**Open: request-provenance ratchet.** Signed reads stage
`cx.request`/`request_streamed_body` unsigned plus `cx.sign_http`. In
`venues/apexomni/src/lib.rs` that rehashes the 2 legacy staging calls and adds
1 `sign_http` site. The ratchet refuses any new site without an
owner-authorized bootstrap, as with `SIGNER_POLICY_BOOTSTRAPS`. This needs a
decision: an authorized rehash, or moving the reads onto managed Read contexts
first.
