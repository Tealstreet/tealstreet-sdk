# Binance identity preflight terminal custody

Base: actual landing `15bd7821b52e378a9e32a60ecb897275c52109a4`.
Branch: `refactor/rust-binance-identity-terminals`.

## Inventory and ownership

The base inventory contains 26 Binance fingerprints / 30 occurrences. Identity
has two, both in the synchronous `finish_unissued` helper: numeric `done` and
`fail`. Its callers only pass errors; the successful identity HTTP path already
uses selected credentials, genuine Read admission and context-bearing completion.

Convert only those identity preflight terminals. Production scope is
`identity.rs` plus the single `FetchAccount` dispatch arm in `lib.rs` if the
obsolete numeric argument is removed. Tests use existing identity and command
entry targets. Prune only the exact identity inventory map and carry CALLERS,
QUALITY and this indexed plan. No engine/harness, account reconciliation,
listen-key/session, HTTP recipe/header/signing, history, trading or Store edits.

The open listen-key PR #1851 lists identity/lib changes, but its identity patch
is credential selection already present on this base. Confirm the narrow
identity rejection helper and dispatch arm with its owner before production
edits. Plan and original controls can be prepared independently.

## Contract

An identity preflight error may be computed locally, but cannot publish a
diagnostic or terminal until the existing `read_context(cx, None)` genuinely
admits the active command. On initial refusal, return immediately: the engine
owns the single correlated failure, with no numeric fallback or extra diagnostic.
On admission, use the existing `identity_error` path to emit the correlated
diagnostic and typed failure, retiring that Read root once. Local failure creates
no HTTP request, signature, session, deadline/release fact or Store mutation.

Keep the selected-credential HTTP path byte-equivalent, including routes,
timestamps, request bytes, credential selection and successful identity shape.
Keep ordinary preflight message text and diagnostic-before-terminal order.
Capability-unsupported products remain refused by the engine before venue entry.
Remove the unused success serialization arm and numeric argument rather than
retaining an unreachable fallback API.

## Ordered implementation and verification

1. Commit this plan before controls or implementation.
2. With original production source, fill the real engine context registry and
   issue supported-product `FetchAccount` commands with missing, disabled or
   invalid credential configurations. Add separate assertion REDs showing these
   local refusals bypass engine Read admission. Expect the engine's exact
   capacity error, one typed Internal failure, no diagnostic or transport, and
   no Store change. Existing valid identity command capacity refusal is the
   working control; keep successful selected-signature identity controls.
3. Verify repeated ordinary local refusals complete once, preserve their legacy
   text and account snapshot, and do not leak Read ownership. Preserve Spot and
   COIN-M capability refusal and native HTTP error/success behavior.
4. Commit original RED controls, then implement the narrow conversion. Run the
   same controls GREEN plus the existing identity/command-entry suites and
   scoped Binance strict checks. No full local workspace suite or release WASM.
5. Remove exactly the two identity fingerprints, leaving all other inventory
   maps byte-identical; total becomes 24 / 28 for Binance. Record old/new map and
   hashes, run the unchanged checker and its controls, and update caller docs.

No push, merge window or readiness is implied by a local plan or RED result.
#1902 remains frozen on its own branch; #1944 stays behind its actual landing.

## Local implementation evidence

Original control commit `84e327e2c8f` has four independent assertion REDs and
seven working GREEN controls, with no production edits. After conversion all
eleven tests pass unchanged. Scoped all-target Binance Clippy with warnings
denied, package formatting, the unchanged inventory checker and all fifteen
ratchet controls pass. The listen-key owner's original command-entry test is
preserved byte-for-byte as a prefix; only new tests are appended.

The identity inventory map becomes empty: remove exactly
`c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548: 1`
and `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa: 1`.
Global occurrences shrink 1,032 to 1,030; every other manifest byte is exact.
Manifest SHA256 changes from
`86e948782fc26b7565e252bda20638f698894b3f9c1298f3fedb0b22c9e7f0ef` to
`3a665d5adaa07307a33a931349a12bc90debcda34e1d50f3c9eeac4c15d887b7`.
Shared engine/harness and the selected HTTP/response tail remain untouched.
Independent implementation review, current integration and full CI are separate
readiness gates; this local result does not claim a merge window.
