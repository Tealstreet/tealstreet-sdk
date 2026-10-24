# KuCoin broker signing role

Plan, 2026-10-24. Lane claude-m26ps4 (credential core), assigned by
claude-ddi2il. API fit for codex-9czdjh's KuCoin dispatch signer
(`RUST_KUCOIN_DISPATCH_SIGNER.md`, plan `0fc81b6d8d1`, not yet on the tip). claude-19pt8s reviews the role table and the setup policy.

The branch is cut from tip `ba3d7da1d3a` for the plan and REDs. Source waits
for and merges the #1990 landing, which composes #1973's (P6b) closed role
table with #1990's declaration rule.

## Problem

KuCoin's optional broker attribution signs `timestamp + partner + apiKey` with
a separate broker secret (HMAC-SHA256, base64). Today:
- `ExtraKey` has only `ZklinkSeed`.
- `LocalSigner::key` refuses role refs, while `HmacSha256` reads `key()`.
So no provider path exists for the broker secret, short of raw REST
credentials, reusing `zklinkSeed`, or inventing an `Identity`. All three are
refused.

## Contract

- **Role.** `ExtraKey::KucoinBroker`, serialised `kucoinBroker`, is one new row
  in #1973's closed table: `ExtraKey::algorithm()` maps it to `HmacSha256`.
  - A broker ref signs only `HmacSha256`.
  - An ordinary ref is never a broker ref.
  - Role refs stay unpresentable, and cannot be interpolated as
    `PayloadPart::Credential`.
  - The provider gains no special case and no rule of its own.
- **Material.** The broker secret is UTF-8.
  - Trusted setup trims it as native does (JavaScript `String.prototype.trim`,
    including U+FEFF).
  - A value that is blank after trimming mints no ref and is not an error,
    because the broker is optional.
- **Selected identity.** Setup mints one broker ref. The primary's
  `Identity.extra` names it. When a read key is configured, the secondary's
  `Identity.extra` names the same ref, an alias with no second copy (as with
  Aster's `signingFromPrimary`).
  - The recipe reads the broker ref from whichever identity `Selection` chose,
    so it never reaches across slots.
  - A secondary that supplies its own broker material is refused.
- **Non-secret parts.** Partner, name and verify are trimmed `EngineOptions`
  values owned by the venue. They never enter the signer. Suppressing a
  partial triplet (missing ref, empty partner or empty name) is the venue's
  rule, in codex-9czdjh's cutover.

## Setup policy changes

- `ExtraKeyPolicy` gains `trim: bool` (default `false`, so ApeX is
  unchanged). With `trim`, the material is trimmed before decoding, and an
  `Optional` field that is blank after trimming is absent.
- `ExtraFieldPolicy` gains `AliasPrimary`, valid only on the secondary slot.
  - The secondary's `extra[key]` becomes the primary's ref when the primary
    has one, and is absent otherwise.
  - Secondary-supplied material for that key is refused.
  - **Approved by claude-19pt8s**, as a declared, always-on share of one
    installed copy (not a fallback), under four conditions:
    - (a) the secondary never supplies its own broker material (refused);
    - (b) no primary broker means the secondary gets none (no error, nothing
      invented);
    - (c) the alias names exactly the primary's role ref, so it carries
      #1990's shared binding and #1973's role semantics;
    - (d) Rust setup, codegen and TS accept `AliasPrimary` only on the
      secondary's extra policy.
- KuCoin construction rules (`kucoin` and `kucoinspot`), in `signerSetup`:
  - primary `extra: {key: kucoinBroker, field: optional, encoding: utf8,
    trim: true}`;
  - secondary `extra: {key: kucoinBroker, field: aliasPrimary}`.
- `build_support` codegen and the TS `LocalSignerSetup` rule types accept
  both. codex-9czdjh's cutover supplies the material in setup.

## REDs (each shown failing first)

The REDs decode the policy and role from JSON, so they fail at runtime (P4/P6b
style) rather than at compile time.

- R1 (crypto): a `kucoinBroker` extra under the KuCoin policy installs one ref,
  and that ref signs `HmacSha256` over `[stamp, partner, Credential(api key)]`
  equal to an independently computed HMAC. Fails today: no such `ExtraKey`.
- R2 (crypto): trimming. `"  b\u{feff} "` installs `b`; `"   "` and `""` mint
  no ref and do not error; without `trim` (ApeX), behaviour is unchanged.
- R3 (crypto): alias.
  - The secondary's `extra[kucoinBroker]` equals the primary's ref, and the
    material is installed once.
  - Secondary-supplied broker material is refused.
  - No primary broker means no secondary broker.
- R3b (crypto): condition (d). `aliasPrimary` on the primary slot is
  refused at setup.
- R4 (after #1990 lands): the role binding. A broker ref refuses every other
  algorithm and `Present`/`Credential`; an ordinary ref cannot sign as a
  broker. This is pinned once the table exists.

## Ownership and order

- claude-m26ps4: plan, REDs, the role row, setup policy, rules and codegen.
- codex-9czdjh: the venue recipe, the partial-triplet rule and the cutover. No
  edits to shared files.
- Order: #1973 and #1990 land, then this PR (merging their landing), then the
  KuCoin cutover.
- Each change gets an independent review answering "per-venue fix of a
  cross-cutting concern?". The answer here is no: one row in the shared table,
  plus two generic setup-policy options that any family can use.
