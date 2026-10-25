# Credential-only trusted setup

Status: PLAN, revision 3, CLEAR (kuhwxi, at `8d5914ee117`). It folds in kuhwxi's reviews (B1-B5, M1-M2, then R1,
X1, X2), fe7lgm's review (points 1-5) and ddi2il's ruling on G1 and G3. No source before CLEAR.
Owner: shared credential core (m26ps4). First consumer: GRVT, whose recipe is
zs5e83's (plan `c13acb15e56` on `feat/rust-grvt-write-funnel-plan`). This plan
does not edit that recipe; G2, G4, G5 and G6 stay open for zs5e83.

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
- **G3** (Empty-key login selection) may revise D3. Under the B3 model, Empty
  gives `{credential: "", signing: ...}`, which is invalid for both
  validators. It gates phase 3.

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
  only present the key without a signature. For an Optional family that
  scope is open (G6).
- Capability admission (`Cx::can_sign`) refuses an absent signing ref as
  `MissingKey`; a missing extra ref stays `InvalidRequest` in any need order.
  An admission refusal does not latch; the venue returns it.
- Unsigned session-authenticated Mutations never call `signing_ref()`, so
  they stay available.
- A family wanting earlier typed admission, before queue effects, does it in
  its own command admission (GRVT: G2).

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
3. D3 + D5: the `Option<Secret>` key, `KeyPresence` and `SetupFacts`. C2, C3
   and C9 go GREEN. **This phase does not start until G3 and G6 are answered** with
   zs5e83. If the answer reopens D3, D3 is revised first.
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

## Open questions for GRVT (zs5e83, when the recipe resumes)

- **G2: placement admission.** Refuse placement typed in GRVT's command
  admission, using `signing_ref()` and the declared Secp256k1Digest
  capability, before queue effects. kuhwxi's view: the D4 `NotSent` floor is
  safe, but GRVT should refuse earlier. Whether admission alone keeps work
  from the latch is part of G6.
- **G3: Empty login (gates phase 3).** Under `KeyPresence::Empty` the slot is
  `{credential: "", signing: None}`, and it fails both validators. Which
  Selection and `session_egress` does the `api_key ""` login use? One option
  is a primary-slot Selection built from the Empty fact with no material. That
  is a further shared-core change and may revise D3.
- **G4: Blank keys.** A whitespace-only key installs a credential ref
  (`trim_credential=false`) and logs in with those exact bytes; identity
  eligibility reads Blank. Confirm the native diagnostic and I/O precedence.
- **G5: legacy callbacks.** Do any GRVT remote, payload or delegated shapes
  need credential-only support? D2 refuses them by default.
- **G6: latch scope for an Optional family (gates phase 3 and GRVT
  adoption).** Today one signed attempt on a credential-only primary latches
  all later signed staging (D4), including key-only presentation and session
  steps, second-slot reads and startup. For an Optional family an absent key
  is a valid state, which contradicts the per-operation check D4 is named
  for. Options:
  admission refuses first so nothing reaches the latch (G2); the latch skips
  `signing_ref()` absence for an Optional primary; or the latch narrows to
  work that carries a signing operation. No decision yet; settled with
  zs5e83 and kuhwxi when the GRVT recipe resumes.
