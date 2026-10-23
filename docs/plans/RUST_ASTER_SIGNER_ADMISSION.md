# AsterDEX signer admission

Plan, 2026-10-23. Lane claude-m26ps4; signer owner and reviewer claude-19pt8s.
Stacked on #1845 (`86ebd9a993c`, the V1/V3/Code `sign::Recipe`), which sits on
#1844 (local secp256k1) and #1839. Unblocks claude-55ao7t's Aster listen-key
session PR. ApeX admission follows with the same shape.

Admission means:
- the family signs only through the injected provider at dispatch;
- the venue holds `signing::Identity` refs, never key material;
- trusted setup installs the material;
- the registry and the rules admit the family;
- one positive remote-signing test covers each half: browser-hosted and
  keyless remote.

Binance (#1351 onward) and Bybit (#1728) are the references; OKX #1773 shows
the current hosted and remote shape.

## Current state (inventory)

| Piece | Binance/Bybit | AsterDEX today |
| --- | --- | --- |
| Rules `construction-rules.json` | `bridge.dispatchSigning`, `remoteSigning{enabled, protectedHeaders, payload…}` | `canSign [[key],[remoteSigner, asterLocalAuth]]`, no dispatch, no remoteSigning, no `signerSetup` |
| Registry arm | refuses raw key/secret, takes `options.signing`, requires a provider | `registry/src/lib.rs:960-985` passes raw `key/secret/asterdex.{private_key, read_key, read_secret,…}`; extra branches at `:444-451` (local rules) and `:744-749` (read-agent validation skip) |
| Venue | `Options.credentials/read_credentials: Option<Identity>`; `cx.sign_http(… Recipe)` at dispatch | `venues/asterdex/src/lib.rs:143-217` builds raw `sign::Credentials`; `request.rs:198-222` and `trading.rs:349-356` sign eagerly; listenKey `account.rs:406-413, 973` |
| Setup | defaults (UTF-8 HMAC key) | V3 needs the wallet private key as `secretEncoding: hex`, often `0x`-prefixed; `KeyEncoding::Hex` rejects `0x` |
| Browser half | `registry/src/hosted_signing.rs:108-120`, `legacySigning.ts:13-26` | absent |
| Remote half | `remoteSignerProvider.ts` HMAC over `ws-auth` | `remoteSignerProvider.ts:262` HMAC only; Aster keyless with `auth.privateKey` stays TS (`rust.keyless-remote-signing.test.ts:45`) |
| Nonce | none (HMAC + timestamp) | V3 process static `sign.rs:231-244` keyed by the signer address; #1845 wraps it as `interim_nonce()` |

## Decisions

1. **Whole-family migration, staged remote scope.** V1, V3 and Code all move
   to `sign::Recipe` at dispatch together (no V1-only venue stage). Positive
   remote proofs cover V1 in both halves. Keyless V3/Code stays an explicit
   typed refusal until P8 (claude-19pt8s) adds the `op` wire; the browser half
   for V3/Code follows the same P8 `Input::Op`.
2. **Setup: KeyEncoding::Hex accepts an optional `0x`/`0X` prefix** (agreed
   with claude-19pt8s). P6b `l1Wallet` and ApeX `zklinkSeed` rely on it
   instead of stripping per family.
3. **Identity mapping** (trusted setup, TS `localSignerSetup.ts` and native
   `prepare_local_signer_with_policy`):
   - V1: `credential` = API key, `signing` = HMAC secret (UTF-8).
   - V3: `credential` = the `X-MBX-APIKEY` value (`key`, falling back to the
     signer address); `signing` = wallet private key (`auth.privateKey` or
     `secret`), declared hex.
   - V3 secondary reproduces today's quirk exactly. Read requests sign with the
     primary wallet key (`lib.rs:205-206`). The secondary slot therefore gets
     `credential` = read key and `signing` = the primary wallet material
     installed again under the secondary slot. Raw `read_secret` is used only
     when no primary wallet key exists.
   - User and signer addresses are public config and stay in `Options`; they
     never enter the identity.
   - No `stable_id` is required.
4. **Version resolution without secrets.** `sign::resolve_version` today looks
   at secret presence. The venue receives a non-secret `signing_present` flag
   (true when the primary identity exists), so resolution is unchanged and no
   material is read.
5. **Nonce key.** V3's venue sequence is per signer address (public), so the
   engine domain keys on `NonceKey::ethereum_address(signer)`: an engine
   `NonceDomain` `ASTERDEX_V3` (ProtocolMs x1000, `u64::MAX`). This corrects
   RUST_CREDENTIAL_SELECTION C3's "remote-signed families require stable_id"
   to "require a non-secret key: a stable id, or a public identifier when the
   venue's own domain is public". The P5 "never an address" line in
   RUST_SIGNER_NON_HMAC_MIGRATIONS goes to claude-19pt8s to reconcile. Until
   `DispatchContext::reserve_nonce` exists (after #1771), the recipe keeps
   `interim_nonce()`; no second allocator.
6. **Ownership** (agreed 2026-10-23).
   - This lane does the identity and signing swap in `lib.rs`, `request.rs`,
     `trading.rs` and `account.rs`. codex-ih35js and codex-cxxyqr hold no work
     there.
   - `history.rs` and `tests/history.rs` stay codex-cxxyqr's. History signs
     only through `request.rs` `signed_streamed`. The one line that needs to
     change, `history.rs:298` (`credentials.is_none()`), waits for cxxyqr's
     answer.
   - The listen-key session authority belongs to claude-55ao7t.
   - Later managed conversions integrate this pin.

## Original REDs (to prove failing before each change)

- R1 registry: asterdex with `options.signing` refs and an injected provider
  is refused ("Injected signing is not supported for this venue yet").
- R2 rules: TS rejects `localSigner` for asterdex (no `dispatchSigning`).
- R3 setup: a `0x`-prefixed hex wallet key fails `KeyEncoding::Hex`.
- R4 browser half: `hosted_signing` refuses asterdex V1 HTTP work.
- R5 remote half: keyless asterdex V1 with a RemoteSigner does not run on the
  Rust engine (or refuses), while Binance does.
- R6 venue: the asterdex venue still holds raw key material, so a test venue
  built with refs-only options cannot sign.

Each fix PR shows its RED on the base before the change.

## Phases (small PRs)

1. **This plan** (docs only), with an independent review and claude-19pt8s's
   signer-owner review.
2. **Setup and registry:**
   - optional `0x` hex (R3);
   - rules `bridge.dispatchSigning`, plus `signerSetup` V3 hex for both slots;
   - the registry asterdex arm takes `options.signing` and refuses raw
     key/secret/private key (R1, R2);
   - TS setup mapping, including the V3 secondary rule;
   - drop the `:444-451` and `:744-749` Aster branches once reads use the
     secondary identity.

   This needs phase 3 in the same merge window, since the venue must accept
   identities.
3. **Venue swap** (this lane, per decision 6):
   - `Options` holds identities plus public V3 config and `signing_present`;
   - every signed request (REST reads, trading, listenKey) goes through
     `Slots::select` (C1 #1817) and `sign::Recipe` (R6);
   - legacy `Query::signed` stays as a `cfg(test)` oracle.
4. **Browser half (V1):** asterdex in `registry/src/hosted_signing.rs`
   families and `legacySigning.ts`, mirroring `registry/tests/hosted_signing.rs`
   and `rust.hosted-provider.wasm.test.ts` (R4).
5. **Remote half (V1):**
   - rules `remoteSigning {enabled, protectedHeaders: ["X-MBX-APIKEY"],
     payloadSupported: false}`;
   - `remoteSignerProvider` hex over `ws-auth`;
   - keyless V3/Code is refused with a typed error before construction;
   - mirror `rust.delegated-provider.wasm.test.ts` and
     `rust.remote-signer-runtime.test.ts` (R5);
   - update `rust.keyless-remote-signing.test.ts:45`,
     `registry/tests/construction.rs:369-381` and
     `rust.construction-rules.wasm.test.ts`.
6. **Nonce:** `ASTERDEX_V3` domain via `DispatchContext::reserve_nonce` after
   #1771 and #1852.
7. **ApeX admission:** the same shape.
   - Rules `signerSetup` primary: `secretEncoding: base64Text`, plus extra
     `zklinkSeed` (`requiredForPrivate`, hex).
   - Setup maps `omniKeySeed` into extras, and `withoutSigningCredentials`
     strips it.
   - HTTP and WS-login HMAC ride the existing wire; zkLink orders stay
     refused until P8.
   - codex-hcmq3f owns the ApeX `lib.rs` identity field.

Each PR:
- CARGO_BUILD_JOBS=2, focused crate and TS tests;
- an independent review answering "per-venue fix of a cross-cutting
  concern?";
- RUST_CUTOVER_CALLERS lines for caller-visible setup changes (V3 keys move
  from `auth.privateKey` options into signer setup);
- no merge without a window.
