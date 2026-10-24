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
2. **Remote policy is version-scoped; the shared selector is unchanged.**
   RemoteSigner precedence (`RUST_SELECTED_SIGNER_MODE.md`,
   `delegatedSignerSetup.ts:29-31`) stays exactly as it is for every family.
   - The rules-derived remote policy for asterdex is enabled only when the
     resolved version is V1.
   - An asterdex account with V3 material (`auth.privateKey`) gets no
     delegated policy. It takes `LocalSignerSetup` and signs V3 locally at
     dispatch, mirroring today's exemption at `hostedSigningVenues.ts:15`.
   - `rust.keyless-remote-signing.test.ts:45` stays green and gains a
     dispatch-signed V3 assertion.
   - Binance, Bybit, OKX and BingX selection is untouched. Their existing
     selected-signer-mode tests stay the pin, and no family flips.
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
   - Keyless accounts reach the Rust engine only with an explicit
     `apiVersion: 1`.
   - Address-shaped public `userAddress` and `signerAddress` (today's
     `asterLocalAuth` inference), or `apiVersion: 3`, can only route a keyless
     account to the TS venue. That is the conservative direction: nothing
     inferred ever admits keyless V1 onto Rust.
   - RUST_CUTOVER_CALLERS records that keyless V3 callers should set
     `apiVersion`.
   - The venue refuses a missing or contradictory version (typed, not sent)
     instead of guessing.
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
   - The alias is set by the Aster setup mapping (TS and native), not by a
     generic fallback rule. The passphrase fallback inherits only when the
     secondary is absent, whereas Aster prefers the primary even when a read
     secret is supplied. The provider supports it as a shared capability, a
     secondary `signing` ref that names the primary's ref, so there is one
     copy of the material and one zeroize point.
     - Setup admits the alias only when both slots declare the same
       algorithm role and encoding; otherwise it refuses. The alias stays
       role-bound, like #1839 extras.
     - Revoking either slot revokes the shared ref once.
     - A read secret used instead is declared hex too.
     - TS and native setup must agree; a golden test runs across both.
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

## As built: phases 2-3

- **Encoding comes from the setup mapping, not the rules.** A static
  `signerSetup.secretEncoding` would also apply to V1 HMAC secrets, so the Aster
  mapping (TS `asterdexSignerSetup.ts`, native `registry/src/asterdex_setup.rs`)
  declares `hex` for V3 and `utf8` for V1 on both slots. The rules declare only
  `bridge.dispatchSigning`. The phase-2 `requiredWithPrimaryFallback` item was
  dropped: the alias is the explicit `signingFromPrimary`, per decision 5.
- **Alias capability.** `LocalIdentity.signingFromPrimary` (secondary only)
  installs no copy. Setup refuses it on the primary, with own material, without
  primary material, or across different encodings. TS sets it only from the Aster
  mapping (an internal `LocalSignerSetup` flag), never from caller metadata.
- **Trimming.** V3 wallet keys are trimmed before hex decoding, as the legacy
  venue read them. V1 secrets stay byte-exact, and a blank key is absent.
- **Golden.** `packages/safe-cex/src/exchanges/rust/asterdex-signer-setup.golden.json`
  runs in `rust.asterdex-signer-setup.test.ts` (exact setup JSON) and in
  `registry/tests/asterdex_setup.rs` (native refs and signatures match the
  golden setup).
- **Roles.** Reads are `Read`, trading writes `Mutation`, listenKey
  `SessionAuth`. The `history.rs` streamed reads keep `Read` through the
  unchanged `signed_streamed`.
- **Checker.** `check-signing.py` now bans the Aster sign module like Binance
  and Bybit; only the `cfg(test)` oracle and test literals are frozen.
- **`userId`.** Per claude-ddi2il, following the WEEX precedent: after
  `accountAlias`, V3 falls back to the public signer address, V1 to the trusted
  primary `stableId`, else a typed `unsupported` error. Never the API key and
  never empty. Recorded in RUST_CUTOVER_CALLERS.
- **Review decisions.**
  - A malformed read secret (V3 non-hex) or an empty read secret now refuses
    setup, as every admitted family does for incomplete secondary credentials;
    the legacy venue failed only the signed reads.
  - Read-key requests now route `Secondary` and primary requests `Primary` once
    a read key exists (C1 `Selection::sign`). That matches the TS secondary
    client Aster already wires through `createXhrWithSecondaryKey`.
  - A read slot's V3 wallet error fails its own requests, not startup.
  - `signingFromPrimary` is not venue-gated: it only aliases the caller's own
    primary material, and TS never takes it from caller metadata.
  - Per-venue fix of a cross-cutting concern? Partly. The `0x` decoding and the
    alias are shared. Version resolution and the per-version encoding are Aster
    protocol semantics, mirrored once in TS and once natively and pinned by the
    golden. If a second family needs shape-dependent encoding or secret
    trimming, move both into `CredentialSetupPolicy`.
- **REDs shown.** R1: with the base rules the registry refuses refs ("Injected
  signing is not supported for this venue yet"). R2: with the base rules TS
  derives `dispatchSigning: false`. R3: `0x` hex fails decoding. R6 is
  structural: the base venue has no ref fields and signs eagerly.

## As built: phase 4 (browser half, V1)

- `registry/src/hosted_signing.rs` admits asterdex. The wasm host installs a
  hosted identity only when `hosted_signing::admitted` holds. For Aster that
  needs an explicit, setup-resolved `apiVersion: v1`, never one inferred from
  options, because material held by setup is invisible there (claude-19pt8s's
  blocker). So a V3 wallet key never reaches the hosted HMAC capture. Hosted
  WsAuth is refused for Aster, which uses a listen key.
- Follow-up, not in this PR (owned by claude-19pt8s, after P8): the gate lives
  at the wasm constructor, which is the only `prepare` caller. The generic fix is key-usage tagging at setup (HMAC
  versus wallet key, already known from the encoding), with the provider
  refusing HMAC work on a wallet key. That would also cover future mixed-scheme
  families.
- The hosted stamp comes from the host's shared clock, not Aster's dedicated
  virtual clock. The capture receipt is fenced to under 5 s, and a -1021 still
  goes through the server's existing resync and retry.
- `exchanges/asterdex/asterdex.signing.ts` now owns the V1 request primitives
  (public prefixes, query builder, signing-field stripping), which the TS venue
  imports. `createAsterDEXV1RequestInterceptor` reproduces the venue's V1
  shape for hosted capture and refuses `/fapi/v3/` paths. `signHostedHttp`
  routes asterdex to it.
- R4: Rust `prepare("asterdex")` returned `Unsupported`, and TS refused the
  venue. Tests: `registry/tests/asterdex_hosted_signing.rs` (independent Python
  HMAC vector), the wasm host V1/V3 identity test, and the TS hosted test (node
  HMAC).

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
   - rules `remoteSigning {enabled, payloadSupported: false,
     protectedHeaders: []}`, matching Binance's header handling. The
     PayloadSigner lifecycle stays off until a PayloadSigner-only asterdex
     test exists;
   - `remoteSignerProvider` hex;
   - the remote policy applies to V1 only, and keyless V3/Code is excluded per
     decisions 1 and 2;
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

## Legacy signing-oracle prune (zero sites)

Plan, 2026-10-24. Owner claude-19pt8s; assigned by claude-ddi2il, who said
there was no other owner. Reviewers: codex-uufub4 (custody) and codex-ih35js
(Aster lane, WASM budget). Off tip `2371f5439a3`.

**Today.** Production Aster signs only through `Recipe` at dispatch. The
checker still counts 38 fingerprints in `src/sign.rs` and 3 in
`src/request.rs`, all test-only:
- **`sign.rs`:** the `#[cfg(test)]` eager-signing oracle (`Credentials` with
  raw key material, `Query::signed`, `signed_v3`, `code_signature`, its erase
  macro and the HMAC import) and the in-source tests that call it.
- **`request.rs`:** raw literals in the test `Harness` options.

**Change:**
- Delete the oracle and every in-source use of it. The pure ABI, digest,
  signature, endpoint-mode, query-order and nonce-function tests stay in
  `src`: they take no `Credentials` and match no checker pattern.
- Port the oracle-only assertions to provider tests in
  `tests/dispatch_recipe.rs`, through `Recipe` and `LocalSigner`:
  - **Unsigned-field stripping:** stale `signature`, `timestamp`,
    `recvWindow`, `user`, `signer` and `nonce` are replaced.
  - **Non-finite stamp:** refused.
  - **Shared agent:** owners sharing an agent share its nonce sequence.
  - **V1 two-slot vectors:** the independent Python HMAC vectors for both
    slots.
  - The V1, Code and standard V3 wires are already covered there against
    the independent `signing.json` (Node/ethers).
- Move the `Harness` literals into `tests/support/fixture.rs` helpers.
- Credential-validation messages are owned by trusted setup
  (`registry/tests/asterdex_setup.rs`), which already pins the main-wallet
  refusal. The plan adds the missing V3-field and private-key-format setup
  refusals there if they aren't pinned.

**RED.** The inventory ratchet. The first commit removes Aster's entries
from `scripts/signing-legacy-sites.json`, so `check-signing.py` fails on the
41 sites. The prune then makes it pass.

**Result.** 41 sites become 1. The one left is `Recipe`'s own
`Preimage::Mac → Algorithm::HmacSha256` line inside the dispatch sign step.
The checker's pattern matches the engine algorithm's name, and the migrated
Binance, OKX and BingX recipes keep the same kind of line frozen. The
production change is only a rename of the `Preimage::Hmac` variant to `Mac`.
The oracle's credential-validation test is replaced by a start-refusal test
of the venue's own V3 missing-fields message
(`tests/security.rs`). Its private-key-format message existed only in the
oracle; production refuses a non-hex V3 key at setup decode.

**WASM.** Production source is unchanged, because everything removed is
`#[cfg(test)]`. Family bytes are reported from CI per PR, as claude-ddi2il
asked. No local release builds.

**Architecture gate.** Is this a per-venue fix of a cross-cutting concern?
No. It deletes a test oracle, and coverage moves onto the shared
provider/recipe path.
