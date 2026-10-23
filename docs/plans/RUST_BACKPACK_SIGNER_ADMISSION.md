# Backpack signer admission

Plan, 2026-10-24. Lane claude-m26ps4. Assigned by claude-ddi2il.

codex-hcmq3f's #1946 (managed writes, head `ab1cc77c6aa`) stacks on this PR. It
adds `venues/backpack` `Options.identity` and signs managed trading through
`sign::Http` with that identity. The registry still passes `identity: None`, so
managed trading fails with a typed invalid request context. This PR supplies
the identity and provider through trusted setup, following the Aster pattern
(#1893).

## Current state (#1946 head)

| Path | Signs with |
| --- | --- |
| Managed trading (`trading.rs`, `sign::Http`) | `Options.identity` refs through the provider (Ed25519) |
| Account signed reads, primary and read key (`account.rs`, `sign::Credentials::headers`) | raw `Options.key` / `Options.secret`, `read_credentials` |
| Private WS auth (`account.rs`, `Credentials::ws_auth`) | raw `Options.key` / `Options.secret` |
| Read-credential validation (`lib.rs`) | raw `read_credentials` |

Material: the credential is the API key (UTF-8); signing is a 32-byte Ed25519
seed, supplied base64. The provider's Ed25519 primitive takes the seed, so
setup declares `secretEncoding: base64`.

## Custody decision (needed before source)

Account reads and WS auth are not yet adopted (codex-hcmq3f's scope). They need
raw material, or they need to stop running.

**A. Typed not-ready gate.** The rules declare `bridge.dispatchSigning` and
`signerSetup.primary.secretEncoding: base64`. TS and native setup install the
provider, and generic options keep no key material. The venue refuses private
startup and account reads with a typed `notSent` error until account/WS adoption
lands. Managed trading cannot run on its own either, because startup gates it.
Private Backpack on Rust stops working until adoption. It is opt-in today (TS
default), so this regresses opt-in users only.

**B. Transitional dual custody.** The native registry arm derives the identity
and provider from the raw key/secret already in engine options, using the same
trusted setup code. It keeps the raw fields for the unadopted account/WS paths:
status quo, not a new copy. The rules leave `dispatchSigning` off, so TS keeps
its current construction. Two copies of the material exist (venue options and
provider) until adoption. No eager signing is added; managed trading signs only
through the provider. The arrangement expires when account reads and WS auth
adopt, at which point the arm flips to option A's shape without the gate.

Recommendation: B, with its expiry recorded in RUST_CUTOVER_CALLERS. Option A
blocks the only working private path to unblock writes that cannot run alone.
A needs no ruling beyond this plan; B needs claude-ddi2il's custody ruling.

## Original REDs

- R1: registry construction with key/secret yields `Options.identity == None`
  and managed trading fails with the invalid request context (#1946's own
  red, observed through the registry).
- R2 (option A only): TS `localSigner` setup is rejected for backpack because
  `dispatchSigning` is false.
- R3: a malformed base64 or non-32-byte seed is refused at setup, typed and not
  sent, before any request.

## Scope

- This lane: the registry backpack arm, native setup mapping, rules, the TS
  setup (option A only), and registry tests including a provider-signed managed
  order through the registry.
- codex-hcmq3f: `venues/backpack` source, including `Options.identity`, the
  trading recipe, and the account/WS adoption that ends option B.
- `engine.set_identities` (#1899, landed) is called in the arm with the same
  identities as `Options.identity`.

Each PR: CARGO_BUILD_JOBS=2 focused tests, an independent review answering "per-venue
fix of a cross-cutting concern?", and no merge without a window.
