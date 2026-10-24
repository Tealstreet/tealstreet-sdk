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
- Plan and REDs only. No source change.

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
