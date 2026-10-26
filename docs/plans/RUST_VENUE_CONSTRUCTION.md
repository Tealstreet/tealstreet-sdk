# Venue construction follow-ups

Landing branch: `feat/rust-rewrite`. Preserve venue behavior; one PR per phase.

1. Expose AsterDEX's existing native API-version resolver for registry validation
   and construction metadata. Send only the caller's explicit version from TS.
   Read native metadata in `canSign` and STP preflight once WASM exists. Preserve
   synchronous pre-host inference, since exchange construction precedes async
   WASM loading. Cover explicit overrides, trimmed auth precedence, flat-field
   fallback, invalid credentials, and no-key public accounts.
2. Derive public mode and signing eligibility in the registry, carrying raw
   caller public-mode intent separately from the legacy derived value. Move
   the explicit-private Paradex refusal with the same production behavior.
3. Move HyroTrader endpoints, Hyperliquid spot routing validation, and private
   non-Paradex payload-signer refusal to Rust. Keep factory routing exceptions
   for keyless remote signing and payload signers. Retain harness polling options.

Verify each phase with focused registry, venue, wasm-host and TS bridge checks;
CI owns compiled WASM, replay and timing gates. Audit each diff, publish review
findings, merge only green CI, and apply the lane's landing/revert guard.

## Phase 2: shared public-mode rules

A JSON table beside the SDK bridge declares signing and private-data conditions.
TS evaluates it synchronously; Rust embeds and evaluates the same file before
credential validation and startup configuration. `publicMode` carries requested
public intent plus raw remote/payload/index presence, alongside the existing
`publicDataOnly` hint. Absent `publicMode`, native/harness options retain their
legacy interpretation. Construction metadata adds optional signing/public bits.

Verification: existing venue-wrapper regressions, focused registry/wasm and
Binance tests, feature-only builds, typecheck/lint, and a compiled parity matrix
for every registered name in CI. Pin remote versus payload signers, Hyperliquid
versus Spot, explicit-private Paradex refusal, and untrimmed string presence.

## Phase 3: remaining host construction rules

Forward raw Hyro production/simulator network URL maps and mode; the registry
selects endpoints with existing fallback semantics. Preserve native explicit
endpoints when that config is absent. Move private non-Paradex payload-signer
refusal before generic native credential validation, keeping Unsupported/NotSent
with a generic typed WASM constructor-error bridge. Keep factory routing and
Paradex signing transport intact. Hyperliquid's old spot refusal is already gone.

Verify raw config forwarding, native selection and precedence, structured
startup/validateAccount refusal before I/O, public and Paradex bypasses, and the
existing factory/spot/polling regressions. Run focused Rust/TS checks and CI.

## Payload-signer guidance follow-up

Clarify that private non-Paradex Rust accounts must remove `payloadSigner`
before using local signing credentials; adding credentials alone still refuses.
Update the native message and mocked/compiled error assertions, preserving
Unsupported/NotSent. Verify focused registry and bridge tests, typecheck/lint,
and green CI; review and run the separate landing guard before merge. Notify
the construction docs owner if the pending refresh quotes the old message.
