# ScalarOnly primary as a login Session root

Engine-only PR from master. Owner: claude-m26ps4. Reviewers: claude-fe7lgm (core) and
codex-zs5e83 (serial call-auth author), with credentials review by claude-m26ps4.
Landing needs a claude-8x5tm1 exact-tree GO. Ruled by claude-ddi2il as the first of
the Paradex PR-3b engine prerequisites. The answer was agreed jointly by fe7lgm and
zs5e83; see "Q1 final" in RUST_PARADEX_REQUEST_CONTEXT.md.

## Problem

`Identity::accepts_session_root` (`engine/src/signing.rs`) binds a credential-bearing
login Session and its serial root to the installed Primary identity by comparing
`credential.reference == self.credential`. For a trusted ScalarOnly primary
(`credential_absent`, as Paradex's Stark key and Lighter's key are), `credential` is
empty, and the only authority is the signing ref (`authority_ref()`).
`Selection::authority` already binds that ref for ScalarOnly Mutations. So a
ScalarOnly login Session, which `Sessions::open` admits with that non-empty ref,
is refused Invalid at every root check (`validate_session_root`).

## Change

One matcher: compare against `*self.authority_ref()`. All other conjuncts stay:
- the Primary slot;
- `validate_credential()`;
- the write equals the Session credential exactly;
- `signing_ref().is_ok()`.

For every credential-present identity `authority_ref() == credential`, so behaviour
is unchanged there, and a signing, passphrase or extra ref still cannot stand in.
Only the trusted ScalarOnly marker binds its signing ref.

The authority ref stays signing-only. The Session presents only what the venue's
login yields (the bearer), never the ref. Dispatch prepare's `signing_only_ref` and
`work.presents` checks already refuse Present and interpolation of a signing-only
ref before any provider callback. There is no fake credential, no public key used as
a credential and no borrowed owner. The engine rechecks its own installed Primary and
the live Session at issue, continuation and resend, as today.

`RUST_SERIAL_CALL_AUTH.md` gains one sentence that separates an authority-bound
credential (a trusted ScalarOnly signing ref, binding only) from a presentable one.

## Originals (RED before source, each through the real `validate_session_root` path)

1. **Genuine ScalarOnly control.** A trusted ScalarOnly primary signs the login on a
   Session whose credential is `Selection::authority(Primary)`. One mixed root then
   issues an unsigned Session call and a signed Preflight call with exactly its
   declared provider work, all under the same root credential. RED on base: Invalid
   before IDs.
2. **Present and interpolation.** A recipe that presents or interpolates the
   authority ref on that root refuses before the provider callback.
3. **Refused substitutes.** A passphrase, extra or Secondary ref as the root or
   Session credential refuses. So do a foreign Session and a same-account stale one.
4. **Ordinary identities unchanged.** A credential-present primary with its signing
   ref as the root still refuses; the existing
   `a_signing_ref_cannot_replace_the_credential_ref` keeps every assertion.
5. **No downgrade.** A write-None root still refuses every Preflight declaration
   for a ScalarOnly primary.

Controls that already pass on base are labelled PRESERVATION.

## Gates

- Scoped engine controls with `serial-plans` and `session-bearer` on, plus the
  feature-off builds.
- Strict all-targets clippy, fmt, the bans, and `check-credential-only.py`.
- Byte deltas: Paradex (which links `session-bearer` without `serial-plans`, so
  expect 0) and the GRVT family.
- No venue changes, no Paradex rows, no caps changed.
