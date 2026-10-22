# Shared hosted signer encoding and credential presentation

Plan, 2026-10-22. Owner: codex-5c57jl. This is a separate shared PR after
actual #1757, before OKX/Bitget hosted admission. Planning branch starts at
landing `0db7d9a03a73ba1ffa16cb3710d71715a4d88de8`; implementation must integrate
actual #1757 and current shared inputs before source changes and final proof.

## Contract

The current hosted bridge presents an API key and formats HTTP HMAC output as
hex. OKX and Bitget require base64 of the raw MAC plus a passphrase header;
their static WS logins also present the passphrase. Extend one declared family
policy and the existing primitive provider/capture path. Do not add another
signing endpoint, venue class dependency, provider queue or secret escape.

- Declare signature encoding and required credential presentations as data.
  Existing Binance/Bybit output stays hex/key-only. OKX and Bitget use raw-MAC
  base64 and passphrase presentation. No output encoding inferred from payload,
  advisory Context, or caller-selected credential references.
- Trusted setup validates the existing #1738 field policy, installs opaque
  passphrase references and resolves absent-only secondary fallback. Explicit
  empty never falls back. Material stays in trusted provider custody; SDK
  options, callback receiver snapshots, engine options and venue state retain
  no passphrase. Presentation values belong only to the current attempt.
- Local and injected providers use the same atomic Present/Sign result. The
  existing capture owns the expected result layout, receipt, header policy and
  raw deadline/calibration revision fences. Validate the entire result before
  formatting or publishing any output. No presentation side map or retained
  material in unsigned recipes. Disposal revokes custody and pending authority.
- Extend the #1757 delegated provider centrally to present only its setup-owned
  passphrase refs. RemoteSigner stays primary-only; required secondary signing
  refuses rather than guesses. No new sign-request/sign-response wire field.
  Existing callbacks keep their labels, outputFormat, extraData and header
  precedence. REST-only PayloadSigner behavior remains declared centrally.
- Hosted signing output may carry internal attempt-scoped presentation results
  to the standalone request-shape codec, which places them in the existing HTTP
  headers. Public signWsAuth still returns only the legacy signature response;
  no passphrase is added to the external WS signing wire.

## Implementation and checks

1. Extend construction policy validation and generated Rust declarations once;
   cover invalid/unknown encodings, missing presentations and unsupported refs.
   Reuse existing setup field policy, not venue-specific normalization.
2. Extend registry hosted work/output and the shared JS provider/capture adapter.
   Keep default-deny result headers, existing bounds, correlation, cancellation,
   single consumption and final fresh-release fences. Coordinate signature/result
   regions with RequestContext #1741; do not change managed claim ownership.
3. Add native and JS tests for **two families**, OKX and Bitget: exact raw-MAC
   base64, key/passphrase from owned primary refs, secondary own/inherited/empty
   cases, malformed/truncated/extra results and header precedence. Preserve
   Binance/Bybit byte-for-byte controls and no-codec-at-import mobile support.
4. Rebuilt actual-WASM SDK tests prove browser/provider HTTP and WS for both
   families without TS venue construction, local fallback, secret/passphrase
   retained in options/receiver, or any external wire addition. Family admission
   remains gated until native recipes are landed; probe shared codec contracts
   without claiming production family startup is complete.
5. Include RUST_CUTOVER_CALLERS.md in this PR; document trusted setup/custody,
   callback provisioning, primary-only remote signing and compatibility bounds.
   Independent architecture/security/correctness review, focused checks, full
   CI and authenticated replay evidence precede a separate exact-file guard.

Architecture gate: a shared encoding/presentation extension used by OKX and
Bitget, not a per-venue implementation of credential custody or signing.
This plan does not enable either family or claim cutover completion.

## First implementation checkpoint (actual #1757)

The plan branch now genuinely integrates #1757 at
`e6382b5cfc038612a15b4e4eade8542a62f2e1e5`. First add shared declared
signature encoding and passphrase-header presentation using existing atomic
Present/Sign values and setup-owned passphrase refs. Registry/capture tests for
OKX and Bitget exercise this common work/format path; Binance/Bybit remain exact
controls. The extension does not change SignResult fields, managed HTTP claims,
RequestContext deadlines, or runtime family admission. Actual SDK/WASM hosted
admission remains a subsequent gate with each family adapter, after both its
native recipe and this extension exist; a pure probe is not startup evidence.

Before final CI/proof, integrate the actual #1759 freshness core and any newer
shared inputs under the normal lane rules. Keep its readonly Store/auth trait
changes intact. Every implementation checkpoint receives independent correctness
and adversarial architecture review before commit/readiness.
