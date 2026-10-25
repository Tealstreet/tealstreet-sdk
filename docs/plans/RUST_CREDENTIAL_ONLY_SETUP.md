# Credential-only trusted setup

Status: PLAN. Review first; no source before CLEAR. Owner: shared credential
core (m26ps4). First consumer: GRVT (zs5e83's recipe, plan `c13acb15e56` on
`feat/rust-grvt-write-funnel-plan`). This plan does not edit that recipe;
decisions that touch it are listed under Open questions for GRVT.

## Problem

Some venues authenticate a session with an API key alone. A signing key is
needed only for operations that sign. GRVT logs in with `api_key`, and its
CancelOrders, CancelSymbolOrders and SetLeverage are session-authenticated
unsigned calls. Only placement needs the secp256k1 scalar.

Trusted setup cannot represent that account today:

1. **No signing ref means an empty-string sentinel.**
   `DecodedIdentity::references` (`sdk-rs/crypto/src/signer.rs`) writes
   `signing: KeyRef("")` when the secret is empty. `Identity::validate()`
   rejects any empty ref, and venues filter identities through it
   (Binance, BingX, Bitget, AsterDEX, ...), so the key-only identity is
   dropped. Treating `""` as "no signing key" would be a fake ref under another
   name.
2. **Setup loses the key-presence facts.** `LocalIdentity.key` is
   `#[serde(default)] Secret`, so an omitted key and `key: ""` both decode to
   an empty key. A whitespace-only key installs as an ordinary credential.
   GRVT's native login checks Option presence (`Some("")` logs in with
   `api_key ""`; `None` does not log in). `fetch_identity` checks
   `trim().is_empty()`. A venue cannot recover either fact from a ref, and it
   must not keep the raw key.

## Goals

- An identity can carry a credential with **no signing ref**, as an explicit
  absence that the type checker enforces at every consumer.
- Trusted setup records one non-secret fact about the supplied primary key:
  `Absent | Empty | Blank | Nonblank`. It is computed before the key is
  consumed, and nothing retains the key.
- Every existing family behaves the same. Their identities still require a
  signing ref, so a key-only setup still installs nothing they can use.

## Non-goals

- No change to `Selection` or `Slots`. They are generic over slot material,
  so a credential-only identity in the primary slot selects for Mutation and
  SessionAuth as it is. Selection never refuses because signing material is
  absent (zs5e83 correction 1).
- No credential-optional identity. An identity still needs a nonempty
  credential ref; an Empty key installs no credential ref (correction 2).
- No remote, delegated or payload-signer credential-only accounts. Their
  existing construction refusals stay.
- No GRVT source. GRVT adopts this in its own recipe.

## Decisions

**D1: `Identity.signing: Option<KeyRef>`, as a direct cutover.**
- `None` means the slot has no signing material. `Some("")` stays invalid,
  exactly as today.
- Serde: `#[serde(default, skip_serializing_if = "Option::is_none")]`.
  External or delegated JSON that omits `signing` means absent; `"signing": ""`
  is still rejected by validation.
- `Identity::validate()` keeps its meaning and requires `Some(valid)`. Every
  existing venue filter therefore still drops a credential-only identity.
- New `Identity::validate_credential()` accepts an absent signing ref but
  still validates credential, passphrase, stable id and extras. Only a family
  that adopts credential-only setup calls it.
- New `Identity::signing_ref() -> Result<&KeyRef, SignError>` returns
  `InvalidRequest` on `None`. The roughly 168 current `.signing` reads (27
  source files) move to it mechanically. The compiler finds each one; none may
  default to an empty ref.
- `CredentialSetupPolicy::apply_to_identities`: "complete" becomes credential
  and signing both present. Passphrase and extra checks keep their current
  boundaries. A credential-only slot is not complete, which is today's
  behaviour for an empty secret.
- `LocalSigner` setup: `references` emits `None` for empty secret material.
  Algorithm declaration, the alias check (`primary.signing` must exist) and
  `install` use the `Option`. `remoteSignerProvider.ts` and
  `delegatedSignerSetup.ts` keep minting a signing ref; their identities are
  never credential-only.
- Rejected alternative: a parallel `PresentationIdentity` slot type. It would
  keep two identity paths and a second selection input.

**D2: `LocalIdentity.key: Option<Secret>`, plus the `KeyPresence` fact.**
- `key` loses `#[serde(default)]`, so omitted means `None`. JS
  `JSON.stringify` already omits `undefined`, so an absent `options.key`
  arrives as `None` and `""` arrives as `Some("")`.
- `pub enum KeyPresence { Absent, Empty, Blank, Nonblank }` lives in
  `safe_cex_engine::signing`. Setup computes it from the **raw** supplied
  primary key, before any `trim_credential` policy. Blank means nonempty and
  empty after Rust `str::trim` (Unicode White_Space); TS uses the existing
  `rustTrim`.
- The setup result carries it as
  `SetupFacts { primary_key: KeyPresence }`, returned next to `Identities` by
  `from_setup_with_policy_and_mode` and the JSON variant. Registry setup hands
  it to venue construction through the same record that carries
  `read_status` and `readCredentialsValid` (D4).
- Existing families ignore `SetupFacts`. Native per-family setup builders that
  read `options.key` (`registry/src/lib.rs`, `asterdex_setup.rs`) keep their
  current presence gates; only the `Secret` wrapping changes.

**D3: signing is checked per operation, not per selection.**
- An operation that signs takes its key through `signing_ref()`. With
  `None`, it fails `InvalidRequest` (`NotSent`) inside its preparation,
  before any provider call or I/O, under the same contract as a missing
  provider ref today.
- Unsigned session-authenticated Mutations never call it, so they keep
  working.
- A family that needs typed admission earlier, before queue or placement
  effects, does it in its own command admission. The shared core supplies
  only the fact and the accessor (see G2).

**D4: one evaluator per setup path, pinned by a parity matrix.**
- Local WASM and native setup compute `KeyPresence` in Rust, inside
  `LocalSigner` setup, from the key they consume.
- The delegated and remote JS setups never produce credential-only
  identities, so they report no fact. A family that declares credential-only
  support refuses them, as today.
- One matrix pins every input × path pair. Inputs: missing key, `""`,
  `"  "`, `"\u{85}"` (NEL, trimmed), `"\u{feff}"` (BOM, kept), `" k "` and
  `"k"`. Paths: native and WASM JSON. TS needs no second evaluator because the
  key goes into Rust setup.

## Original tests (RED before GREEN)

Shared-core REDs, each failing on `ad0b74afdf8` for the intended reason:

| ID | Test | Fails today because |
| --- | --- | --- |
| C1 | Key-only `LocalSetup` installs an identity with `credential = Some`, `signing = None`; `validate_credential()` is Ok and `validate()` is Err | `signing` is `KeyRef("")`; there is no absent state |
| C2 | Omitted key and `key: ""` give `KeyPresence::Absent` and `Empty`, with no credential ref for either | serde default collapses them |
| C3 | Presence matrix from D4 across the native and WASM-JSON setup paths | no fact exists |
| C4 | `signing_ref()` on a credential-only identity returns `InvalidRequest`; a Sign prepare built from it publishes no Sign effect and sends no HTTP | no accessor; an empty ref reaches the provider |
| C5 | Unsigned SessionAuth and Mutation selections over a credential-only primary return Primary with `session_egress` granted | the identity is filtered before selection by every current builder (control on `Slots` alone) |
| C6 | JSON round-trip: an omitted `signing` deserializes `None`; `"signing": ""` fails validation; serialization of `None` omits the field | the field is mandatory |

Behaviour-neutral controls, green before and after:

- K1: every family's existing key-only, secret-only, public-only and complete
  setup tests are unchanged. That covers the `credentials.rs` assertions, the
  MEXC trim policy, OKX, Bitget and WEEX passphrase rules, the AsterDEX
  mapping, Backpack PublicRead, the ApeX extra and the Polymarket L1 wallet.
- K2: the alias secondary (`signing_from_primary`) still refuses a missing
  primary signing ref.
- K3: the delegated and remote setup TS tests are unchanged, and
  `credentialPresence` and `derivePublicMode` are unchanged.

## Phases

1. Plan review (this document). REDs C1-C6 come only after CLEAR.
2. D1: the Option cutover, with all consumers through `signing_ref()`, as one
   commit. Gates: workspace all-targets check, engine feature modes, the
   K1/K2 controls, bans and WASM sizes for every family plus combined.
3. D2/D4: `Option<Secret>` key, `KeyPresence` and `SetupFacts` through the
   registry construction record, with C2/C3 GREEN.
4. Docs: a `RUST_CUTOVER_CALLERS.md` row (external providers may omit
   `signing` for credential-only identities only in families that declare
   it), plus the `RUST_SIGNER.md` identity contract.

Each phase needs an independent review answering: "Is this a per-venue fix of
a cross-cutting concern?"

## Open questions for GRVT (zs5e83, when the recipe resumes)

- G1: **policy declaration.** Does GRVT need a construction-rules field
  saying "this family accepts credential-only primaries", or does the venue
  simply call `validate_credential()`? The recommendation is a declared
  `IdentitySetupPolicy.signing: Required | Optional` (default `Required`), so
  the registry refuses credential-only setups for every other family by
  policy rather than by a filter.
- G2: **placement admission.** Refuse placement typed in GRVT's command
  admission using `signing_ref()` plus the declared `Secp256k1Digest`
  capability, before queue effects (recipe step 5). Or is the D3 preparation
  refusal enough?
- G3: **Empty login.** With `KeyPresence::Empty` there is no credential ref,
  so no identity, so no SessionAuth selection. Which selection or
  `session_egress` does the `api_key ""` login use? Option: a primary-slot
  Selection constructor taking a `KeyPresence::Empty` fact and no material.
  That needs a shared-core change.
- G4: **Blank.** A whitespace-only key installs a credential ref
  (`trim_credential=false`), logs in with those exact bytes, and identity
  eligibility reads `Blank`. Confirm that matches native diagnostics and I/O
  precedence.
- G5: **legacy callbacks.** Does any GRVT remote, payload or delegated
  construction need credential-only support? This plan assumes no.
