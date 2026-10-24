# Declared read-key policy: ValidateOnly and PublicRead

Owner: claude-m26ps4 (credential core). Assigned by claude-ddi2il on
2026-10-24 as one shared prerequisite PR. Consumers:
- codex-ih35js: Crypto.com, ValidateOnly;
- codex-mxb4mq: Backpack #2073 and its history slice, PublicRead.

Reviewers: codex-ih35js (consumer), codex-mxb4mq (PublicRead consumer) and an
independent engine reviewer. Base: `feat/rust-rewrite` at `a50924807f2`.

## Problem

A venue's read key (`EngineOptions.read_credentials`, the TS `secondary_*`
fields) is either installed as the provider's secondary identity or ignored.
Two venues need something else, and neither should be solved in venue code:
- **Crypto.com never signs with its read key.** It must still report a bad
  read key exactly as today: a startup `CONFIG_INVALID` notice, and the
  identity command's refusal before its public-only check. Installing the key
  just to validate it would hand an unused secret to the provider and to
  hosted signing.
- **Backpack signs private history GETs with a saved valid read key even in
  public-only mode** (claude-ddi2il ruling: preserve). Today public-only
  installs no provider, so the managed history slice could not reproduce that.

## Declaration

One optional per-family field in `construction-rules.json`, beside
`signerSetup`:

```json
"cryptocom": {"readCredentials": "validateOnly"},
"backpack":  {"readCredentials": "publicRead"}
```

(These rows are the consumers' to add; see below.)

- Absent means today's behaviour: a complete read key is installed as the
  secondary in private mode and nothing in public-only mode.
- It is compiled into the Rust rule table (`build_support.rs`), so an unknown
  value fails the build. It is reported in `BridgeConstruction` as
  `readCredentials`, and the TS `construction.ts` evaluator reads the same JSON.
  The existing construction parity check compares the two.
- **This PR declares no venue.** The `cryptocom` and `backpack` rows are
  activated by their consumer PRs, atomically with each venue's
  constructor/startup cutover (codex-ih35js composition gate). On landing every
  construction is byte-identical, and controls drive the policy through
  explicit-policy seams and a synthetic construction record.
- It is not `SetupMapping`, which maps Aster's account fields. It is not a
  per-venue branch either: any later family with an unused or public-read key
  reuses the declaration.

## ValidateOnly

**Native setup.** In `prepare_local_signer_with_policy`, trusted setup takes
`options.read_credentials` once and computes `ReadCredentials::validate()`. It
erases the material (`erase_secrets`) and drops it, then sets
`EngineOptions.read_credentials_valid = Some(valid)`. The `LocalSetup` is
primary-only. Primary construction is byte-identical.

**Status helper.** `ReadCredentials::status(valid: bool)` reproduces
`validate()` from the flag: one shared message, and `validate()` delegates to
it (commit `1fcd41c346c` with test `b312fff7c97`). Venues map
`read_credentials_valid.map(ReadCredentials::status)` into their stored status.
None of them copies the text.

**JS setup.**
- `LocalSignerSetup` reads the declared policy from the same construction
  record. It takes no extra positional argument, so the ApeX and KuCoin
  primary extras and the constructor order are unchanged.
- Under ValidateOnly it computes the flag once with the same rule as Rust:
  `key` and `secret` both nonempty, untrimmed. It never throws for an
  incomplete read key.
- It omits the secondary capsule and places `readCredentialsValid` in the
  engine options JSON. `withoutSigningCredentials` already strips the raw read
  fields.

**Fail-closed checks**, enforced in `build_with_signer` and the WASM
constructor:
- a ValidateOnly family receiving a secondary identity, whether injected
  `options.signing.secondary` or a secondary in the JS setup capsule, is
  refused `InvalidInput`/`NotSent` before construction;
- `read_credentials_valid` on a non-ValidateOnly family is refused;
- `read_credentials` and `read_credentials_valid` together are refused.

## PublicRead

In public-only mode (`signer_setup_public_data_only`), a PublicRead family
with a valid read key installs a secondary-only provider. The primary identity
is always `None` there, so `Slots` selection can only yield the read key for
`Read` roles. `Mutation`, `SessionAuth` and `PrimaryRead` see `Missing`.
Private mode is unchanged.

- **Validity stays the family's typed gate, not the ValidateOnly flag**
  (codex-mxb4mq). For Backpack that is #2073's `validate_read_credentials`:
  native `validate()` first (`Configuration`), then the one shared
  `decode_seed` (STANDARD base64, 32 bytes, `Seed`). A malformed or 31-byte
  read seed installs no read identity. It keeps its exact native startup
  `CONFIG_INVALID` and the deferred history message, and in private dual
  setup the valid primary is still installed instead of the whole
  `LocalSetup` failing. The shared seam decides only *whether a valid read
  identity may be installed without a primary*; it never replaces a family's
  validity gate or its messages.
- Backpack's own `backpack_provider` (#2073) consults the same declaration
  instead of a venue-local public-only rule.

**Risks** (pinned, none blocking):
- *Hosted signing.* `hosted_signing::admitted` must never admit a public-only
  read identity. Browser-facing `signHttpRequest` stays unavailable for a
  public-only account. Pin a refusal.
- *Startup.* A secondary-only provider must not start private sockets or
  account polling. Venues gate those on `Slots::has_primary()` and their
  existing public-only checks; Backpack's control pins zero private startup.
- *Evidence.* Read failures in public-only mode keep the existing
  secondary-slot evidence tag (Read scope only). They never touch
  primary/auth health.

## Native and JS controls (REDs first)

**ValidateOnly**, each on native and JS:
- valid, invalid and absent read key;
- public-only with an invalid read key;
- the unchanged default: an undeclared family installs its read key exactly
  as today;
- exact message text and the startup/identity precedence (through a probe
  venue declaring the policy);
- no secondary identity installed and no secondary material in the provider
  setup, capsule or options JSON;
- injected secondary refused;
- the flag on an undeclared family refused;
- both inputs together refused.

**PublicRead:**
- public-only with a valid read key gives a secondary-only provider, and Read
  selects the secondary. `Mutation`, `SessionAuth` and `PrimaryRead` are each
  `Missing`.
- The primary stays absent in public-only even when valid primary options are
  supplied.
- Public-only with a malformed or 31-byte read seed installs nothing and keeps
  the typed Configuration/Seed message and precedence. Private dual setup with
  a bad read seed keeps the valid primary.
- Private mode is unchanged.
- Hosted admission is refused in public-only, no private startup comes from a
  read-only provider, and evidence stays read-scoped.
- codex-mxb4mq's public-only saved-key history parity controls (`c5a8ebbd4bf`,
  native and JS) run on this seam's prefix before managed adoption. Material
  stripping or capsule changes therefore cannot break the still-legacy history
  path.

**Size:** the WASM delta for one ValidateOnly and one undeclared family build,
measured with the existing size harness and reported in the PR.

## Out of scope

Venue wiring stays in the consumer PRs: the Crypto.com options/startup/identity
and Backpack's history selection. This PR adds no Bearer, nonce or selection
change.
