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
| Rules `construction-rules.json` | `bridge.dispatchSigning`; Binance `remoteSigning{enabled, payloadSupported, payloadRestOnly, protectedHeaders: []}` | `canSign [[key],[remoteSigner, asterLocalAuth]]`, no dispatch, no remoteSigning, no `signerSetup` |
| Registry arm | refuses raw key/secret, takes `options.signing`, requires a provider | `registry/src/lib.rs:960-985` passes raw `key/secret/asterdex.{private_key, read_key, read_secret,…}`; extra branches at `:444-451` (local rules) and `:744-749` (read-agent validation skip) |
| Venue | `Options.credentials/read_credentials: Option<Identity>`; `cx.sign_http(… Recipe)` at dispatch | `venues/asterdex/src/lib.rs:143-217` builds raw `sign::Credentials`; `request.rs:198-222` and `trading.rs:349-356` sign eagerly; listenKey `account.rs:406-413, 973` |
| Setup | defaults (UTF-8 HMAC key) | V3 needs the wallet private key as `secretEncoding: hex`, often `0x`-prefixed; `KeyEncoding::Hex` rejects `0x` |
| Browser half | `registry/src/hosted_signing.rs:108-120`, `legacySigning.ts:13-26` | absent |
| Remote half | `remoteSignerProvider.ts` HMAC over `ws-auth` | `remoteSignerProvider.ts:262` HMAC only; Aster keyless with `auth.privateKey` stays TS (`rust.keyless-remote-signing.test.ts:45`) |
| Nonce | none (HMAC + timestamp) | V3 process static `next_nonce` (`sign.rs:325-338`) keyed by the signer address; #1845 wraps it as `interim_nonce()` (`:422`) |

## Decisions

1. **Whole-family local migration, staged remote scope.**
   - V1, V3 and Code all move to `sign::Recipe` at dispatch together; there
     is no V1-only venue stage.
   - Remote-signing admission covers V1 only.
   - Keyless V3/Code keeps today's TS venue until P8 ships recipe ids
     `asterdex.v3.1` (EIP-191) and `asterdex.code.1`. `runsOnRustEngine`
     excludes it, so nothing is refused and there is no regression.
2. **Local material wins over remote (shared rule).** `selectDelegatedSigningMode`
   chooses delegated mode only when no local signing material exists. This is
   one rule for every family, not an Aster branch: Hyperliquid and Paradex
   need the same.
   - An asterdex account with `auth.privateKey` keeps `LocalSignerSetup` and
     signs V3 locally at dispatch.
   - `rust.keyless-remote-signing.test.ts:45` stays green and gains a
     dispatch-signed V3 assertion.
3. **Setup: KeyEncoding::Hex accepts an optional `0x`/`0X` prefix** (agreed
   with claude-19pt8s). P6b `l1Wallet` and ApeX `zklinkSeed` rely on it. The
   `sign.rs:430-431` "strip 0x first" comment is updated.
4. **Version resolution moves to trusted setup.** `sign::resolve_version`
   needs the key's shape (`is_private_key`: 64 hex digits), not just its
   presence. Trusted setup (TS `LocalSignerSetup` and native prepare)
   resolves V1/V3 from the material and passes it as an explicit non-secret
   `api_version`.
   - A shared P4 public-derivation fact, not an Aster flag.
   - The V1 "API Wallet keys need the Main Wallet Address" hint and the V3
     32-byte check move into setup validation (hex decoding enforces the
     length).
   - The venue never calls `Credentials::validate()` on material again.
   - Keyless accounts state V3 with an explicit `apiVersion`.
5. **Identity mapping** (setup, both TS and native):
   - V1: `credential` = API key (required), `signing` = HMAC secret (UTF-8).
   - V3: `credential` = `key` (required), `signing` = the wallet key
     (`auth.privateKey`, else `secret`), declared hex.
   - Public config stays in `Options`:
     - `signer` = `auth.signerAddress`, else `key`;
     - `user` = `auth.userAddress`, else `application_id`.
   - V3 secondary reproduces `lib.rs:193-210` exactly:
     - `credential` = read key;
     - `signing` = an alias of the primary signing ref when the primary
       wallet key came from `auth.privateKey`, else the read secret (today it
       never falls back to `secret`);
     - signer = `auth.signerAddress`, else the read key;
     - user = the primary user.
   - The alias is a shared setup policy, `signing: requiredWithPrimaryFallback`
     in `IdentitySetupPolicy` and TS setup, extending the passphrase
     mechanism. It means one copy of the material and one zeroize point.
   - No `stable_id` is required.
   - `public_data_only` with a read key keeps signing GETs with the read
     identity (`lib.rs:193` is ungated today), stated in the family's
     `SlotPolicy`.
6. **Nonce key.** The venue replay domain is the public agent/signer address
   across users (`sign.rs:160`), so the domain is `ASTERDEX_V3`
   (ProtocolMs x1000) keyed by `NonceKey::ethereum_address(signer)`.
   RUST_CREDENTIAL_SELECTION C3 and RUST_SIGNER_NON_HMAC_MIGRATIONS P5 are
   reconciled in this PR (claude-19pt8s approved). Until
   `DispatchContext::reserve_nonce` exists (after #1771), the recipe keeps
   `interim_nonce()`; there is no second allocator.
7. **Ownership** (agreed 2026-10-23).
   - This lane does the identity and signing swap in `lib.rs`, `request.rs`,
     `trading.rs` and `account.rs`. codex-ih35js and codex-cxxyqr hold no
     work there.
   - `history.rs` stays codex-cxxyqr's. They approved exactly one line,
     `history.rs:298`, changing to `!self.credentials.has_primary()`; the
     PublicTrades exemption stays.
   - The listen-key session belongs to claude-55ao7t. It stacks on the venue
     swap and consumes the recipe. listenKey is a signed REST call, not
     `ws-auth`.

## Original REDs (each shown failing on the base before its fix)

- R1 registry: asterdex with `options.signing` refs and a provider is refused
  (`registry/src/lib.rs:691`).
- R2 rules: TS rejects `localSigner` for asterdex (`rust.exchange.ts:615-618`).
- R3 setup: `0x`-prefixed hex fails (`key_material.rs:41-50`).
- R4 browser half: `hosted_signing.rs:118` returns `Unsupported` for asterdex
  V1 HTTP work.
- R5 remote half: keyless V1 with a RemoteSigner stays on TS
  (`isKeylessRemoteSigned`, pinned at `rust.keyless-remote-signing.test.ts:41`);
  the RED is the flipped expectation for V1 only.
- R6 venue: a dispatch test where the venue signs only through an injected
  recording provider and the test asserts no raw material reaches the venue.
  It fails today because the venue signs eagerly with raw credentials.

## Phases (small PRs)

1. **This plan** (docs only), plus the C3/P5 doc reconciliation. Independent
   review and claude-19pt8s's signer-owner review.
2. **Setup and registry:**
   - optional `0x` hex (R3);
   - `signing: requiredWithPrimaryFallback`;
   - setup-time version resolution and validation (decision 4);
   - rules `bridge.dispatchSigning` and `signerSetup` V3 hex for both slots;
   - Cargo feature `dispatch-signing` for asterdex in `registry/Cargo.toml`
     and `hosts/wasm/Cargo.toml`;
   - the registry arm takes `options.signing` and refuses raw
     key/secret/private key (R1, R2);
   - TS stops forwarding `privateKey/readKey/readSecret`
     (`rust.exchange.ts:930-940`), and `withoutSigningCredentials` strips
     `auth.privateKey` while keeping the addresses;
   - remove the `:444-451` and `:744-749` Aster branches;
   - update `registry/tests/construction.rs:369-384`.

   Lands together with phase 3.
3. **Venue swap:**
   - `Options` holds identities plus public config and `api_version`;
   - every signed request (REST reads, trading, listenKey POST/PUT/DELETE)
     goes through `Slots::select` (C1 #1817) and `sign::Recipe` (R6);
   - legacy `Query::signed` stays as a `cfg(test)` oracle;
   - the `history.rs:298` line;
   - prune the asterdex rows from `signing-legacy-sites.json` and run
     `check-signing.py` with its test.

   Tests:
   - V3 standard;
   - Code POST `/fapi/v3/order`;
   - Code GET `/fapi/v3/builder|agent`: a secondary read-key header paired
     with the primary wallet signature;
   - V3 with `0x` material;
   - listenKey through the recipe;
   - `public_data_only` with a read key.
4. **Browser half (V1):** asterdex in `registry/src/hosted_signing.rs`
   families and `legacySigning.ts`, mirroring `registry/tests/hosted_signing.rs`
   and `rust.hosted-provider.wasm.test.ts` (R4).
5. **Remote half (V1):**
   - rules `remoteSigning {enabled, payloadSupported, payloadRestOnly,
     protectedHeaders: []}`, matching Binance, since the API key header is
     presented the same way;
   - `remoteSignerProvider` hex;
   - keyless V3/Code excluded per decision 1;
   - delegated V1 has no secondary (`withoutDelegatedSigning` strips it), so
     GETs use the primary; a test covers remote V1 with a read key supplied;
   - mirror `rust.delegated-provider.wasm.test.ts` and
     `rust.remote-signer-runtime.test.ts` (R5).
6. **Nonce:** the `ASTERDEX_V3` domain via `DispatchContext::reserve_nonce`
   after #1771 and #1852.
7. **ApeX admission:** the same shape.
   - Rules `signerSetup` primary: `secretEncoding: base64Text`, plus extra
     `zklinkSeed` (`requiredForPrivate`, hex).
   - Setup maps `omniKeySeed` into extras, and `withoutSigningCredentials`
     strips it.
   - HTTP and WS-login HMAC ride the existing wire; zkLink orders stay on the
     existing path until P8.
   - codex-hcmq3f owns the ApeX `lib.rs` identity field.

Each PR:
- CARGO_BUILD_JOBS=2, focused crate and TS tests;
- an independent review answering "per-venue fix of a cross-cutting concern?";
- RUST_CUTOVER_CALLERS lines for caller-visible setup changes (V3 keys move
  from `auth.privateKey` construction options into signer setup; setup
  resolves `apiVersion`);
- no merge without a window.
