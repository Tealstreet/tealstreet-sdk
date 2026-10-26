# Lighter secret-only setup principal

Status: shared API proposal, before inert originals and source. Owner:
codex-ih35js. Credential reviewer: claude-m26ps4; core reviewer:
codex-uufub4. Base: #2204 head 01cdbacc21200d37eb0c5ddf7b5b06544dc74763.

## Purpose and dependency

Native Lighter accepts a signing secret with an account selector and API-key
index while the key string is absent. Generic local setup deliberately leaves
the credential ref empty in that case. A managed Required Primary must instead
have a real provider-backed nonsecret principal without pretending the caller
supplied an API key. This prerequisite adds that trusted setup shape; the venue
cutover remains separate.

#2204 already supplies AuthRecipe, its public WS URL, and Schnorr public-key
support. Do not duplicate them. This work leaves SchnorrDigest implemented()
false until the venue cutover deletes the eager holder. It does not enable
dispatch signing for a family, advertise hosted remote support, change a nonce
domain, or change financial values or sends.

## Proposed exact carrier

The crypto provider owns a typed LighterPrincipal containing an API-key index,
validated as 0..=254. It accepts no arbitrary text. An optional account index is
not necessary for installation: native account discovery can resolve the actual
account later. The eventual TxRecipe/AuthRecipe Account and its exact API-key
index, plus the installed signing ref, bind that resolved authority.

The proposed constructor is:

```rust
LocalSigner::from_setup_with_lighter_principal(
    setup: LocalSetup,
    policy: CredentialSetupPolicy,
    public_data_only: bool,
    principal: LighterPrincipal,
) -> Result<(LocalSigner, Identities, SetupFacts), SignError>
```

This is a trusted setup constructor, not a new LocalIdentity JSON field. It
requires the primary SchnorrDigest declaration and refuses a secondary. The
registry invokes it only under an explicit SetupMapping::Lighter declaration;
the shared prerequisite adds the mapping kind but does not activate either
Lighter family row. Bare family-name matching is not the policy.

Compute SetupFacts from the original key before decoding or normalization.
Omitted and null remain Absent; an empty string remains Empty; whitespace
remains Blank; a nonblank value remains Nonblank. The principal never changes
those facts, the cached public/private intent, or a supplied stable_id.

A nonblank supplied key keeps the ordinary credential path and takes precedence.
For absent, empty or blank keys, install the principal only when a primary
signing ref exists and public_data_only is false. Its provider-issued opaque
credential reference is distinct from the signing ref. Provider state retains
the typed index; no principal material goes into the ordinary key map.

Present, Credential interpolation, MAC/signing, public-key and address operations
on the principal ref refuse. This stays true if trusted insertion is attempted
under that ref while it is a principal. remove revokes it. The principal is
never a nonce subject, address proof, stable_id, wire credential, or public-key
derivation source. Signing continues to use only the declared signing ref.

The constructor is feature-gated by schnorr; feature-off setup refuses instead
of installing a dead private principal. Ordinary setup and every other family
remain unchanged.

## Registry and consumer fit

The native and private-JSON registry adapters derive the typed principal from
the account options' exact API-key index. A private signing setup with an invalid
index refuses atomically. Public-only and unsigned setup do not mint a principal
and do not require an otherwise unused private index. No caller-supplied text
is converted into an opaque ref, and the venue never creates Identity/KeyRef.

The later venue cutover declares SetupMapping::Lighter for lighter and
lighter-robinhood. Its JS setup captures a validated public L1 address before
withoutSigningCredentials strips key. That public metadata retains native
lookup/identity candidates, including application-index precedence; it is not
the setup principal. Do not copy a scalar into routing metadata.

For injected hosted providers the trusted provider must issue an equivalent
non-presentable principal under its own provisioning contract. This prerequisite
does not fabricate hosted refs or enable unsupported remote recipes.

## Original controls, before source

Freeze inert stubs and genuine provider requests, then execute discriminating
REDs with ordinary setup as the working control:

1. Secret-only Required setup yields a valid Primary with a separate installed
   signing ref and a provider-backed principal; Schnorr capability remains gated.
2. Absent, null, empty and blank key facts survive exactly. A nonblank supplied
   key keeps its existing credential and Present behavior.
3. Present and Credential payload expansion on the principal refuse; its ref
   cannot sign, derive a public key/address, or become a nonce/stable subject.
4. The original signing ref still signs the independent Schnorr vector through
   the provider. The principal never changes account/API-index transaction data.
5. Invalid index, wrong declaration, secondary, malformed scalar and feature-off
   refuse without installing partial material or changing ordinary families.
6. Public-only and no-private-material setups do not acquire a principal.
7. Native and private-JSON mapping preserve key facts and reject arbitrary
   principal fields; undeclared families retain the old empty-credential shape.
8. Consumer controls retain secret-only account lookup and key-supplied L1
   lookup after stripping, with key-present precedence and no private work in
   public-only mode. Their venue assertions are in the separate cutover.

## Verification and landing

Get m26ps4's credential and uufub4's core review of this API before source.
Execute frozen original REDs, implement the carrier, then repeat the scoped
crypto/registry modes, strict checks, format, bans and ratchets. Retain ordinary
setup and feature-off controls. Run one local composed combined measurement
before READY; CI records affected family sizes. Any generic engine change also
needs the ruled Bybit/Bitget pair and a separate reviewed scope change.

Wait for #2204's actual landing, plain-merge the landing tip, then publish one
small shared PR with exact review and one qualifying PR-triggered run. No job
rerun, force push, capability activation, venue migration or budget change here.
