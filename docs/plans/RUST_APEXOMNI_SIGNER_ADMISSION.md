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
- **Credential presence** is checked on raw fields: `account.rs:575, 1227`,
  `history.rs:361` and `trading.rs:677`.
- **Trading** is converted by #1891 (managed writes, signing at send time
  through the provider). It adds `Options.identity`.

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
   - **Seed check:** `validate_seed` at start becomes
     `Cx::can_sign(primary, ZkLinkContract on the zklinkSeed role)` (#1899).
     A missing or unsupported role is typed not-ready before any I/O.
   - **Presence checks:** each becomes "a complete `identity` is present"
     (credential, signing, passphrase and the zklinkSeed role).
   - **Erasure:** `EraseSecrets` and `Sensitive` are removed from `Options`,
     since nothing secret remains. That removes the `lib.rs` fingerprints.

## Typed not-ready gate

Until a family path is admitted, a private request with no identity or no
provider refuses typed (`NotReady`/`unsupported`, `notSent`) before any I/O.
This covers keyless and remote accounts until P8 #1926. Public data is
unaffected. There is no raw-material fallback and no dual custody.

## Custody

The zkLink seed exists only as the provider's `ZklinkSeed` role. It is
role-only for `ZkLinkContract`, and #1973's closed role table composes with
it. No seed, secret or passphrase bytes reach the venue, logs or `Debug`.
Remote signing of zkLink (P8) stays out of scope.

## Dependencies and order

- **#1891.** This PR changes `trading.rs`'s presence check and removes the
  `Options` fields that #1891's trading still reads at its pin `43ee893a9f1`.
  #1891's current head `6eaca6d94d4` contains that pin. So this lands after
  #1891, stacked on its landed head, and touches `trading.rs` only where #1891
  leaves raw-field reads behind.
- **Landed:** #1841 recipes, #1899 `can_sign`/`set_identities`, #1964
  Backpack pattern.
- **Unaffected:** #1990 key typing, since ApeX signs HMAC on an undeclared ref
  plus the `ZklinkSeed` role.
- **Not here:** P8 #1926 remote zkLink, and keyless private ApeX.

## REDs (existing API, runtime failures, shown failing first)

- **R1 (registry):** `build("apexomni", key, secret, passphrase, seed)`
  installs no provider and no identities today. It must install both, with
  the identity's `zklinkSeed` role present.
- **R2 (refs-only account read):** `build_with_signer` with a provider and
  refs, but no raw fields, cannot sign `/v3/account` today; it reports missing
  credentials. It must release a request whose `APEX-SIGNATURE`, `APEX-API-KEY`,
  `APEX-PASSPHRASE` and `APEX-TIMESTAMP` equal an independent HMAC vector.
- **R3 (refs-only WS login):** the private login frame through the provider
  equals the legacy `ws_login` bytes for the same material.
- **R4 (gate):** a refs-only account without the `zklinkSeed` role refuses
  private start typed, with no HTTP. Today it fails on the raw seed check
  with an untyped error.

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
