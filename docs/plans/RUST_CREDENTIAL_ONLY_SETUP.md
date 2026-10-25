# Credential-only trusted setup

Status: PLAN, revision 4. Phase 2 (D1 + D2, D4 `MissingKey`) landed as #2132 (`a2c914e2b24`).
Revision 4 records zs5e83's G2-G6 answers (GRVT recipe plan `4e6c4f4f3bd`,
`deaa151acb1`) and replaces the open G3 and G6 with D3's typed login intent and
D6's per-operation Optional absence. ddi2il disposed revision 4 APPROVED on
this design, effective at kuhwxi's CLEAR; any later design change goes back to
ddi2il. Revision 3 was CLEAR (kuhwxi, at `8d5914ee117`).
Owner: shared credential core (m26ps4). First consumer: GRVT, whose recipe is
zs5e83's (plan `c13acb15e56` on `feat/rust-grvt-write-funnel-plan`). This plan
does not edit that recipe; G2, G4 and G5 are recorded as GRVT constraints.

## Problem

Some venues authenticate a session with an API key alone, and need a signing
key only for the operations that sign. GRVT logs in with `api_key`. Its
CancelOrders, CancelSymbolOrders and SetLeverage are session-authenticated and
unsigned; only placement needs the secp256k1 scalar.

Trusted setup cannot represent that account today (state at `ad0b74afdf8`).

**1. An absent key becomes an empty-string sentinel.**
`DecodedIdentity::references` (`sdk-rs/crypto/src/signer.rs`) always returns
an `Identity` when a `LocalIdentity` is supplied. It writes `KeyRef("")` for
empty decoded key material (`credential`) and for empty secret material
(`signing`). `CredentialSetupPolicy::apply_to_identities` checks only
complete slots, so incomplete slots are kept. A key-only setup therefore
installs `{credential: "primary.credential", signing: ""}`. What happens next
depends on the crate:
- The 19 crates that filter through `Identity::validate()` drop it. These
  include Binance, BingX, Bitget and AsterDEX.
- The 8 crates that never call `validate()` receive it: apexomni,
  breakoutprop, grvt, hyperliquid, lighter, orderly, paradex and polymarket.
  Of these, apexomni, lighter and orderly read `Identity.signing`, once each.
- Registry construction (`registry/src/construction.rs:210,233`) reads
  `id.signing.0.is_empty()` for `Input::Secret` and `AsterLocalAuth`.

Treating `""` as "no signing key" would be a fake ref under another name.

**2. Setup loses the key-presence facts.** `LocalIdentity.key` is a
`#[serde(default)] Secret`. An omitted key, `"key": null` and `"key": ""` all
decode to an empty key, and a whitespace-only key installs as an ordinary
credential. GRVT's native login checks Option presence: `Some("")` logs in
with `api_key ""`, and `None` does not log in. Its `fetch_identity` checks
`trim().is_empty()`. A venue cannot recover either fact from a ref, and it
must not keep the raw key.

## Goals

- **Explicit absence:** an identity can carry a credential and **no signing
  ref**. The absence is explicit, private to the type, and reachable only
  through accessors.
- **Declared acceptance:** whether a family accepts such a primary is declared
  setup policy, accepted only at registry construction. It is never a venue
  filter. Required families keep today's terminals; refusing key-only for them
  is left to the ApeX fix lane and later per-family PRs (ddi2il ruling).
- **Key-presence fact:** trusted setup records one non-sensitive fact about the
  supplied primary key, `Absent | Empty | Blank | Nonblank`. The fact is
  computed before the key is consumed, and nothing keeps the key.
- **No behaviour change:** every existing family keeps its current terminals.
  That is pinned for both validating and non-validating crates (K1, K4, K5).

## Non-goals

- **No selection change.** `Selection` and `Slots` are generic over slot
  material, so a credential-only primary selects for Mutation and SessionAuth
  as it is. Selection never refuses because signing material is absent.
- **No credential cutover.** `credential: KeyRef` keeps `""` as its
  credential-absent sentinel (B3). That is deliberate:
  - Credential-only is a valid state that some family accepts.
  - Credential-absent is never valid for any family.
  - A slot with `credential = ""` fails both `validate()` and
    `validate_credential()`.

  Cutting `credential` over too would move every secret-only terminal for no
  consumer.
- **No credential-only on remote, payload-signer or delegated paths.** The G1
  policy and B1 refuse them; this decides kuhwxi's G5 view for those paths.
  GRVT may still raise legacy callback shapes under G5.
- **No GRVT source.**

## Decisions

**D1: `Identity.signing` becomes private `Option<KeyRef>`, in one direct
cutover.**
- The field is private. Access goes through:
  - `Identity::new(credential, signing: Option<KeyRef>, ...)`;
  - `signing_ref() -> Result<&KeyRef, SignError>`, which returns
    `MissingKey` on `None`, so a missing primary key feeds the #2109 signing
    latch;
  - `signing_opt() -> Option<&KeyRef>` and `set_signing_ref()`, used only by
    setup, construction resolution and the alias check.

  No consumer can write `unwrap_or_default()` or build a fresh empty ref (fe7
  point 3, B4). Serde keeps working through `#[serde(default,
  skip_serializing_if = "Option::is_none")]` on the private field.
- **Bans backstop:** outside tests, `check-bans.sh` rejects
  `KeyRef(String::new())` and `KeyRef("".into())` / `KeyRef::from("")`, except
  the two validation-metadata sites (`signer.rs` passphrase and extra
  placeholders, about lines 319 and 324). Those never touch `signing`.
- `signing_opt()` and `set_signing_ref()` must be `pub` across crates, so
  `check-credential-only.py` (run by `check-bans.sh`) rejects any call outside
  `crypto/src/signer.rs`, `engine/src/signing.rs`,
  `registry/src/construction.rs` and `#[cfg(test)]` modules or `*_tests.rs`
  files (kuhwxi X2). It also keeps `validate_credential()` inside
  `engine/src/signing.rs`. Otherwise they become the escape hatch the private
  field closes.
- **`validate()` keeps its meaning:** it requires a valid `signing`, so
  validating families still drop the slot. `validate_credential()` accepts an
  absent `signing` but still checks the credential, passphrase, stable id and
  extras.
- **Setup:** `references` emits `signing: None` for empty secret material.
  The algorithm declaration, the alias check and `install` use the `Option`.
  `apply_to_identities` keeps "complete" meaning both refs are present, so it
  keeps today's boundaries.
- **Construction resolution:** `Input::Secret` and `AsterLocalAuth` read
  whether `signing_opt()` is a nonempty ref. A pinned test shows that `None`
  and today's `Some("")` give the same `Resolved` (K5).
- **Migration:** consumers move to `signing_ref()` mechanically, and the
  compiler finds each one. The count of `Identity.signing` reads is smaller
  than a raw grep, which also matches `io.signing`, entropy and JS fields
  (M1). It is not a gate.

**D2 (G1, decided): declared signing policy; Optional is accepted at registry
construction, and Required is unchanged.**
- **The field:** `IdentitySetupPolicy.signing: SigningPolicy`, either
  `Required` (the default) or `Optional`. Serde gives it a default and skips it
  when it equals `Required`. The TS construction-rules `IdentitySetupPolicy`
  gains `signing?: 'optional'` and emits it only when it is Optional. A
  round-trip test pins that every existing policy serializes byte-for-byte as
  before (M2).
- **Required is today's behaviour, byte for byte** (kuhwxi R1, option (a)).
  Setup installs the slot exactly as now, with `signing: None` where it
  writes `""` today. The slot fails `validate()`. A validating crate filters
  it, and a non-validating crate receives it, as now. There is no new setup
  refusal: a user who supplies only a key sees the same terminal in every
  family (K1, K4, K5).
- **Optional:** setup installs the same slot. Only an Optional family's
  construction accepts it, through `validate_credential()`, which the registry
  calls when it hands the slot to construction. Acceptance therefore comes
  from declared policy, never from a venue filter (fe7 point 1).
- **External boundary (B1, C8 narrowed with kuhwxi):** remote, delegated and
  hosted `Identities` JSON that omits `signing`, or sends `null`, is refused
  for every family at the options parse, with today's "Invalid engine
  options" terminal. No family is Optional yet, so per-family acceptance waits
  for the first one. Secondary slots always require `signing`.

**D3: the key-presence fact, `LocalIdentity.key: Option<Secret>`.**
- `key` drops `#[serde(default)]`. An omitted key and `"key": null` both mean
  `None`, which is `Absent`.
- The WASM entry takes setup as a JSON string (`signer_setup_json:
  Option<String>`, `hosts/wasm/src/lib.rs`). `JSON.stringify` already omits
  `undefined`, so an absent `options.key` arrives as `None` and `""` arrives as
  `Some("")`.
- `pub enum KeyPresence { Absent, Empty, Blank, Nonblank }` lives in
  `safe_cex_engine::signing`.
- **How it is computed (B5):**
  - from a borrowed `&str` of the raw `Option<Secret>`, before any
    `trim_credential` policy;
  - Blank means nonempty and empty after `str::trim` on that borrow, using
    Unicode White_Space;
  - there is no owned or trimmed copy outside `Zeroizing`;
  - non-UTF-8 raw input is `Nonblank`, because decoding refuses it later.
- **`SetupFacts { primary_key: KeyPresence }`:** it covers the primary only
  and holds only the enum. Its `Debug` and serialized forms are the variant
  name, and it is never formatted into diagnostics beyond that.
  `from_setup_with_policy_and_mode` and the JSON variant return it beside
  `Identities`. The registry hands it to venue construction on the record that
  already carries `read_status` and `readCredentialsValid`.
- **Existing families** ignore `SetupFacts`. Native per-family builders that
  read `options.key` (`registry/src/lib.rs`, `asterdex_setup.rs`) keep their
  presence gates; only the `Secret` wrapping changes.
- **Typed login intent (G3, decided).** A named shared seam turns trusted
  `SetupFacts` into a non-secret `PrimaryLogin`, never an `Identity` or
  `KeyRef`:
  - `Absent` gives no login intent; the venue keeps `config.invalid` and
    sends no login.
  - `Empty` gives `PrimaryLogin::Empty`. `Slots::new(Some(PrimaryLogin::Empty),
    None, SlotPolicy::default()).select(SessionAuth)` gives the Primary
    selection and its `session_egress`, as native selects over `Some("")`
    today. The venue renders its known `api_key ""` login with Policy
    credential `None`, a real keyless Session with Primary `private_egress`,
    and the existing managed unsigned login: no `Present`, no provider batch,
    no empty-ref validation exception.
  - `Blank` and `Nonblank` give `PrimaryLogin::Installed`: the installed
    credential-only primary, whose login `Present`s the actual credential
    bytes, never trimmed.
  - `SetupFacts::primary_login(&self) -> Option<PrimaryLogin>` is the only
    constructor. It is trusted setup output, never parsed from caller JSON.
  - D3 never makes a credential-absent `Identity` valid and never fabricates
    a provider ref.

**D4: signing is checked per operation, not per selection, on every path
(B4).**
- Every operation that signs takes its key through `signing_ref()`. That
  covers the HTTP sign prepare, the socket SessionAuth login Sign ops and the
  dispatch-time signing work.
- With `None`, the operation fails `MissingKey` (`InvalidInput` / `NotSent`,
  "Signing key is not installed") before any provider call, connect, send or
  HTTP. That is primary material, because setup requires a secondary's
  signing ref (`Identities::primary_lacks_signing`), so it latches signing
  (#2109) on every path, beside a secondary too (C4l): one `config.invalid`
  notice, `StartupFailed` during startup, a socket role that never
  reconnects, and every later signed staging refused, including steps that
  only present the key without a signature. That latch is for Required
  families and for refs setup declared installed; an Optional family's
  intentional absence is D6.
- Capability admission (`Cx::can_sign`) refuses an absent signing ref as
  `MissingKey`; a missing extra ref stays `InvalidRequest` in any need order.
  An admission refusal does not latch; the venue returns it.
- Unsigned session-authenticated Mutations never call `signing_ref()`, so
  they stay available.
- A family wanting earlier typed admission, before queue effects, does it in
  its own command admission (GRVT: G2).

**D6 (G6, decided): intentional absence in an Optional family refuses only
the signed operation.**
- **Disposition (ddi2il):** a deliberately absent signing ref in an Optional
  family refuses that operation with a typed reason, `NotSent`, and no
  account latch. Unsigned operations (key-only `Present` login and refresh,
  session-authenticated unsigned work, second-slot reads) and startup stay
  available. Required families keep #2109's `MissingKey` latch unchanged.
- **Shape:** `apply_to_identities`, on the Optional branch that accepts a
  credential-only primary, records the intentional absence on the `Identity`
  as a private flag. It has `serde(skip)`, so injected JSON can never set it
  (C8 still refuses injected slots without `signing`). `signing_ref()` then
  returns a new `SignError::SigningNotConfigured`, with venue error
  `InvalidInput` / `NotSent` and its own static message. Every other absence
  stays `MissingKey`.
- **Where the flag comes from (kuhwxi carry A, ddi2il ruling option 2):**
  the flag is never transported, and `serde(skip)` drops it on any hop.
  Because C8 refuses injected identities without `signing` for every family,
  a credential-only identity reaches an engine only through trusted local
  setup in the engine-owning realm. The flag is derived there, by
  `apply_to_identities` from the family's trusted `IdentitySetupPolicy`. It
  is set only on an installed primary whose `signing` is `None`, and never
  from anything in a payload. Phase 3 adds no external acceptance surface.
  Accepting an Optional credential-only identity across a serialized hop,
  and its round-trip RED, belongs to the hosted/injected GRVT step, and only
  if that step proves it needs one (zs5e83's G5: the intended path is the
  in-realm hosted `LocalSetup`).
- **Injection (carry B):** JSON carrying the flag's field name cannot set
  it. `Identity` ignores it, and a pin fixes that behaviour so a later serde
  change can't open it.
- **Equality (carry C):** the flag takes part in `Identity`'s `PartialEq`, so
  a flagged and an unflagged identity never compare equal (`primary_owns`,
  `matches_identity`); a pin fixes this.
- **Dispatch:** `SigningNotConfigured` is a per-request preparation failure.
  It never sets `credential_refused`, never latches and never emits
  `config.invalid` or `StartupFailed`. `Cx::can_sign` returns the same typed
  error at admission.
- **Not allowed:** suppressing every `MissingKey` because the policy is
  Optional, mapping it to a generic error, or a venue exception. A real
  provider `MissingKey` for a ref that setup installed still latches per
  #2119 (inline `sign_ready`), Optional or not.
- **Serial Plans (fe7lgm via zs5e83):** the operation refuses typed
  `NotSent`; a serial command's terminal follows its Plan journal, prior
  effects included, with no whole-command `NotSent` overclaim.
- **Sockets:** a failed signed socket step still stops that operation and
  does not reconnect in a loop. An Optional family's own login is unsigned
  (D3), so the latch-free refusal never touches it.

**D5: one evaluator per path, with a parity matrix.**
- Local WASM and native setup both compute `KeyPresence` in Rust, from the key
  they consume.
- Delegated and remote JS setups never produce credential-only identities
  (D2), so they report no fact. TS needs no second evaluator.

## Original tests (RED before GREEN)

Shared-core REDs. Each fails on `ad0b74afdf8` for the stated reason.

| ID | Test | Fails today because |
| --- | --- | --- |
| C1 | An Optional family's key-only `LocalSetup` installs `signing = None`; `validate_credential()` is Ok, `validate()` is Err | `signing` is `KeyRef("")`, and there is no policy |
| C2 | An omitted key, `"key": null` and `"key": ""` give `Absent`, `Absent` and `Empty` | serde default collapses them |
| C3 | Presence matrix: absent, `null`, `""`, `"  "`, `"\u{85}"` (NEL, trimmed), `"\u{feff}"` (BOM, kept), `" k "`, `"k"`. Run through native setup and the WASM JSON entry TS actually calls | no fact exists |
| C4 | `signing_ref()` on `None` refuses an HTTP sign prepare. Custody: the signing reservation and the Context are released (`Serial::signs` and the planned-write reservation), the terminal is `MissingKey` (`InvalidInput` + `NotSent`), a peer signed command on the same registry still signs and releases (the installed primary signs and a secondary exists, so nothing latches), and there is no Sign effect or HTTP | no accessor; an empty ref reaches the provider |
| C4s | A socket SessionAuth login that signs, given `None`, refuses before send and latches: one `config.invalid`, `StartupFailed` with `MissingKey` during startup, no Sign or WsSend, the socket closes, and no second connection for the role (#2109 latch) | same; today `sign_failed` on WsSend closes the socket and the supervisor reconnects with backoff |
| C4l | With a secondary installed, an HTTP sign prepare for a credential-only primary's `None` latches once (`config.invalid`, `MissingKey`) | today `Some("")` latches through `primary_owns`; a step's own `MissingKey` beside a secondary did not |
| C4p | Sequential Mutation Plan (`http-plans`): a signed call settles Accepted, then a signing call with `None` fails `MissingKey` in preparation. The projection keeps the settled call's facts, with no Rejected or NotSent overclaim | same |
| C5 | Control on `Slots` alone: Mutation and SessionAuth over a credential-only primary choose Primary, and `session_egress` is granted | (control) |
| C6 | Identity JSON: omitting `signing` gives `None` and serializing `None` omits the field. `"signing": ""` fails `validate()` and `validate_credential()` | the field is mandatory |
| C7 | A Required family's key-only setup gives today's pinned terminal, with no venue code change. Only an Optional fixture family receives a slot that `validate_credential()` accepts at construction | no policy |
| C8 | Remote, delegated and hosted identities without `signing`, or with `null`, are refused for every family at the options parse with today's terminal | after D1 they would parse as `None` |
| C9 | `SetupFacts` `Debug` and serde output contain only the variant name | no type |
| C10 | Login intent: `primary_login()` maps Absent to `None`, Empty to `PrimaryLogin::Empty` and Blank/Nonblank to `Installed`. `Slots<PrimaryLogin>` over Empty selects Primary for SessionAuth with `session_egress`. Pins: Empty with and without the scalar and with secondary metadata, exact login body and headers, Prepare Primary and the final Bind/Claim. Absent keeps `config.invalid` with no login | no seam |
| C11 | Optional absence: a test that deliberately bypasses G2 reaches absent-ref preparation and fails `SigningNotConfigured` (`InvalidInput` / `NotSent`). There is no `config.invalid`, `StartupFailed` or latch; a real login `Present` plus unsigned cancel and leverage still succeed afterwards, including during startup | today it latches through `MissingKey` (C4s/C4l path) |
| C11p | Serial Plan: an unsigned call settles, then the absent-ref placement fails in preparation. The operation and command terminals are pinned, there is no account latch, and later unsigned work succeeds | same |
| C11i | Optional family with an installed ref: a provider `MissingKey` on inline `sign_ready` still latches (#2119), and C4s/C4l stay unchanged for Required families | control: green before and after |
| C11r | Serialization boundary (carry A, option 2): (a) a serialized Optional credential-only identity is still refused at the options parse (C8, every family); (b) trusted local setup derives the flag only for an installed primary with `signing` `None` under Optional policy, giving `SigningNotConfigured`; the same setup under Required gives `MissingKey`; a serialized copy never carries the flag back | no flag or derivation |
| C11j | Injection and equality (carries B, C): JSON naming the flag's field leaves it unset (pinned as ignored), and a flagged identity is not equal to an otherwise identical unflagged one | no flag |

Behaviour-neutral controls, green before and after:

- **K1:** every family's existing key-only, secret-only, public-only and
  complete setup tests are unchanged. This covers the `credentials.rs`
  assertions, the MEXC trim policy, the OKX, Bitget and WEEX passphrase rules,
  the AsterDEX mapping, Backpack PublicRead, the ApeX extra and the
  Polymarket L1 wallet.
- **K2:** the alias secondary (`signing_from_primary`) still refuses a missing
  primary signing ref.
- **K3:** the delegated and remote setup TS tests, `credentialPresence` and
  `derivePublicMode` are unchanged.
- **K4:** for apexomni, breakoutprop, grvt, hyperliquid, lighter, orderly,
  paradex and polymarket, pin the key-only and secret-only setup terminal:
  kind, outcome and message before and after. For each crate either pin it,
  or show that the crate never consumes `Identity`. For apexomni, pin the
  terminal after the X1 fix lane (its admit refusal of an empty signing ref),
  not today's reconnect loop. This control depends on that lane.
- **K5:** `construction::resolve` gives the same `Resolved` for a `None` and a
  `Some("")` signing, for `Input::Secret` and `AsterLocalAuth`.
- **K6:** the IdentitySetupPolicy round-trip from M2.

## Dependency: ddi2il's socket sign-failure fix lane (X1, kuhwxi's)

Today a socket login whose signing fails non-transiently goes through
`engine.rs` `sign_failed` on WsSend. That closes the socket and calls
`on_close`, so the venue supervisor reconnects with backoff indefinitely; ApeX
reconnects about every 30 seconds. The fix lane:
- makes such failures (Unavailable, or an invalid ref) an engine-level
  terminal with a notice;
- adds an ApeX admit check that refuses an empty signing ref.

The lane landed as #2109 (`9bba5373dbb`): only `SignError::MissingKey` on
primary material latches. Phase 2 merges it, so C4s and the apexomni K4 row
(`apexomni_key_only`, now building `signing: None`) assert the post-lane
terminal.

## Phases

1. Plan review (this document). C1-C9 REDs come after CLEAR.
2. D1 + D2, with D4's `MissingKey` refusal: the private `Option` cutover,
   the signing policy, the external boundary and the bans rule. C1, C4-C8
   (with C4l) and K1-K6 go GREEN.
3. D3 + D5 + D6: the `Option<Secret>` key, `KeyPresence`, `SetupFacts`, the
   typed `PrimaryLogin` seam and the Optional `SigningNotConfigured` refusal.
   C2, C3, C9, C10 and C11 (with C11p and C11i) go GREEN. Originals start
   after kuhwxi CLEARs revision 4. Gates add Bybit and Bitget sizes on the
   composed tree.
4. Docs: a `RUST_CUTOVER_CALLERS.md` row saying external providers may omit
   `signing` only for Optional families, and the `RUST_SIGNER.md` identity
   contract.

Every phase's gates:
- workspace all-targets check;
- engine feature modes;
- bans;
- WASM sizes for every family and combined, against the base.

Each phase also needs an independent review that answers: "Is this a
per-venue fix of a cross-cutting concern?"

## GRVT answers (zs5e83) and recorded constraints

G3 and G6 are decided above (D3, D6).

- **G2 (GRVT owns):** placement and replacement check the selected signing
  ref and the declared Secp256k1Digest capability in command or Preflight
  admission, before the queue, builder work, provisional Store effects or any
  cancel preceding a replacement. There is no blanket Mutation gate: key-only
  CancelOrders, CancelSymbolOrders and SetLeverage stay unsigned and
  available. A typed unreleased failure belongs to that owner; record any
  changed pre-queue failure order. Preparation still rechecks (D6); admission
  is not a substitute.
- **G4:** native `private_start` returns first for `public_data_only`. Login
  respects login_pending, authentication_failed and login_retry before the
  key check. `None` gives `config.invalid` with no login; `Some("")` and
  whitespace serialize exactly and log in on Primary, never trimmed.
  `identity.rs` refuses public-only, `None`, empty and trim-blank before the
  request. Classification uses `str::trim` before policy (NEL versus BOM).
  Scalar presence never manufactures credential eligibility.
- **G5:** no GRVT legacy remoteSigner, payloadSigner or delegated callback
  needs credential-only support; those boundaries stay (D2 refuses them). The
  hosted `LocalSetup` with an API key and no scalar is the intended
  credential-only path; caller-supplied external `Identities` JSON is not.
