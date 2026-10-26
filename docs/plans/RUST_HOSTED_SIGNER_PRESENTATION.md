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

Phase 1 uses one `RemoteSigningPolicy` snapshot for signature encoding,
passphrase-header naming and the existing callback-header precedence. HTTP work
retains key/MAC at result slots 0/1 and appends a Present-only passphrase at slot
2 when declared. It requires the setup-resolved ref before provider execution;
it does not resolve absent fallback or invent material. The formatter enforces
exact slot counts, nonempty UTF-8/header-safe presentation and the final combined
header budget. WS keeps its existing two-result signature-only contract. The
OKX and Bitget declarations remain unenabled/default-deny; tests exercise their
common pure work/formatter without opening the hosted/runtime admission gates.

## Phase 2 provider checkpoint

Reuse the setup-field validation/absent-only fallback helper for local and
callback providers. The trusted delegated setup consumes an optional passphrase
into provider custody and exposes only a distinct opaque Present ref; inherited
secondary passphrases reuse the setup-resolved primary ref. No material enters
engine identities, SDK options or callback receiver snapshots.

Keep a passphrase Present map separate from public-key interpolation. A request
may present it only in a fully validated batch containing a Sign operation for
an owning identity (including a setup-authorized alias); a Credential payload
part cannot name it. Validate the entire batch before any callback. Missing or
empty required material refuses before callback/I/O; no wire response is expected
to supply a passphrase. The unchanged remote wire requests the declared hex or
base64 format; base64 must be canonical padded encoding of exactly 32 MAC bytes,
including pad-bit checks. No format inference from context or payload.

Two-family provider/helper tests cover strict encoding, independent golden MACs,
missing/cross-slot/ref-interpolation refusal, setup fallback, callback count,
cancellation and scrubbed custody. Existing Binance/Bybit wire/receiver/import
controls remain unchanged. This phase changes neither native dispatch/release
ownership nor runtime family admission. Keep actual hosted SDK acceptance as a
mandatory family gate, with no claim from these lower-level probes.

Phase 2 checkpoint evidence: both families originally failed four provider/setup
assertions before the implementation. The corrected shared path passes 217
focused tests across ten SDK suites, including unchanged Binance/Bybit wire,
receiver/import/runtime and refusal controls. Package typecheck/lint pass.
Independent correctness and adversarial reviews are clear. Runtime admission
remains disabled, and no actual-WASM or deployed-family claim follows from these
lower-level tests. The next gate is fresh full CI/replay provenance on the final
integrated head before this shared extension is ready to merge.
