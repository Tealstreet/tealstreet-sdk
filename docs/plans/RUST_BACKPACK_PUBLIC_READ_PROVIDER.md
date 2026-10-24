# Backpack public-only read-key-only provider

Owner: claude-m26ps4 (seam). Consumer: codex-mxb4mq (Backpack managed history,
`backpack` row). Assigned by claude-ddi2il on 2026-10-25 under the PRESERVE
ruling: in public-only mode, a saved valid read key keeps signing Backpack's
three private history GETs.

## Stack and status

- Local branch `feat/rust-backpack-public-read-provider`, not pushed. It
  stacks on:
  - #2078 (`825d203d5d3`, the shared read-key policy);
  - #2073 (`9db50a1ecb7`, Backpack's real secondary identity and typed read
    status);
  - codex-mxb4mq's parity control `c5a8ebbd4bf`.
- #2073 and #2078 are not landed. The branch is re-composed on their actual
  landings before any push.
- Plan and REDs at `268b3288497`. The source seam is committed locally after
  codex-mxb4mq's plan CLEAR (see below).

## Gap

#2078 adds the shared PublicRead declaration: in public-only mode no primary
is installed and only Read roles can select the read key. Backpack does not
use the shared setup, though. Its own `backpack_provider` (#2073) returns no
provider at all when `public_data_only` is set. A public-only Backpack account
therefore has no managed signing identity for history. Today's history keeps
working only through the legacy raw helper, which the managed history slice
removes.

## Seam

`backpack_provider` consults `construction::read_credentials_policy("backpack")`.
When the account is public-only and the family declares PublicRead:
- The primary is never built, even if valid primary options are supplied.
- The read identity is installed only when #2073's typed gate passes
  (`validate_read_credentials`: native `validate()`, then the one shared
  `decode_seed`), giving a provider that holds only that identity.
- An invalid read key installs nothing and keeps the exact typed
  Configuration/Seed status and message.

Undeclared, public-only behaviour is unchanged (no provider), and private mode
is unchanged. No other file changes: the venue already selects with
`Slots(identity, read_identity, default)` and `Read { scope: None }`, so Read
gets the read key and Mutation, SessionAuth and PrimaryRead see `Missing`.

The `backpack` row (`"bridge": {"readCredentials": "publicRead"}`) stays with
codex-mxb4mq's managed-history consumer and is activated atomically with it.

## REDs and controls (`registry/src/backpack_public_read_tests.rs`)

These declare PublicRead through #2078's test-only seam.
- **RED:** public-only, with or without valid primary options, installs a
  provider holding only the valid read identity. Today it fails at "the read
  identity".
- **RED:** that provider's identities give `Read` the Secondary slot, and
  `Mutation`, `SessionAuth` and `PrimaryRead` `Missing`. Today `Read` is
  `Missing`.
- **Control:** a public-only malformed or 31-byte read key installs nothing,
  keeps the typed status ("Invalid Backpack base64 seed" or "Ed25519 seed must
  be 32 bytes") and installs no primary.
- **Control:** undeclared, public-only installs no provider, as today.
- **Control:** private dual setup is unchanged, and a bad read key keeps the
  valid primary.
- **Gate:** `registry/tests/backpack_public_only_history.rs` (`c5a8ebbd4bf`, 2
  tests, three routes) stays GREEN on this stack. The JS/capsule parity gate
  belongs to codex-mxb4mq.

Scoped (`--no-default-features --features backpack`, JOBS=4): 2 RED, 3 GREEN,
gate 2/2, strict clippy.

## Pins for source

- No hosted admission for a public-only Backpack account
  (`hosted_signing::admitted` stays false).
- No private startup from a read-only provider.
- Read evidence stays secondary/Read-scoped.
- Each of these is a control with the source.

## Source (local)

`backpack_provider` checks the declared policy. Undeclared public-only returns
no provider, as before. Declared PublicRead in public-only mode never builds or
probes the primary (`primary_allowed`) and installs only a read identity that
passes the typed gate.

Controls added for codex-mxb4mq's source pins:
- A **real signature**: the provider's inline Ed25519 signature under the read
  identity equals an independent `safe_cex_crypto::ed25519::sign` over the
  same read material.
- A malformed primary in public-only mode is never decoded.
- Configuration precedes Seed: an empty key gives the native validate message.
  Absent, empty, malformed and 31-byte reads are each typed.
- No hosted admission, and no signing work at public-only startup.

Results:
- 7/7 seam tests, all 27 library tests, #2073's admission tests 11/11 and the
  parity gate 2/2.
- Strict clippy is clean.
- Mutation check: allowing the primary in public-only mode fails 4 tests.
- Read-scoped evidence for managed history is a control in codex-mxb4mq's
  history adoption, where the read actually issues.

Independent review: CLEAR, no P1/P2, with three P3 notes.
- **Duplicated rule.** It answered yes to "per-venue fix of a cross-cutting
  concern?": Backpack restated the shared PublicRead primary rule. Now one
  helper, `public_read_excludes_primary`, serves both the shared setup and
  `backpack_provider`. Backpack passes its resolved `public_data_only`, the
  same flag its venue arm reads. Backpack's own provider remains until its
  dual custody is removed; then it moves to `prepare_local_signer_with_policy`.
- **Startup test.** It is a guard, not proof (it passed before the seam), and
  is labelled so.
- **Signature oracle.** It uses the same Ed25519 primitive, so it proves the
  installed key material, not the curve code. That is acceptable for its
  purpose.
