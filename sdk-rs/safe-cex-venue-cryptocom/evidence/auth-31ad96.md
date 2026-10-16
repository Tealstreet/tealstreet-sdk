# Auth draft audit: 31ad96e4b8

Audited SHA: `31ad96e4b8e487c2d600875534c5dd0e6a99dba6`.
Direct parent: `0e7798978ddc25f4338b331bebb2b3888d591a41`.
Inherited integration base: `115f2f1c82077057764e859d6980d6c955a32315`.
This report is recorded in a later commit; it was not present at the audited SHA.

Python 3.12 tomllib over git-show contents of every Crypto.com manifest at the
SHA measured **8 ported / 13 divergent / 116 pending**, 137 total. No manifest
rows changed against the inherited integration base. generateSignature,
generateAuthenticatedRequest and generateWsAuthMessage remain pending.
`git ls-remote origin refs/heads/feat/cryptocom-auth-functions` confirmed the
full audited SHA before any follow-up push.

## Measured gate

With a clean tracked tree at the audited SHA:

```sh
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --test auth --jobs 1 --locked
```

Exit **101**, six E0277 compilation errors: String versus &Value comparisons
in auth.rs at lines 144, 149, 182, 183, 226 and 234. No auth test executed.
This is not a runtime mutation failure. Local log: `.cryptocom-auth-31ad96.log`.

NOT RUN at this SHA: full scoped tests, serializer test, mutation controls,
process-count gate, parity indexer, extractor writer, format check and both
strict Clippy gates. The intended distinct Clippy invocations are:

```sh
/opt/homebrew/opt/rustup/bin/cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --all-targets --jobs 1 -- -D warnings
/opt/homebrew/opt/rustup/bin/cargo clippy --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-cryptocom --target wasm32-unknown-unknown --all-targets --jobs 1 -- -D warnings
```

The earlier scoped host check exited 0 on a moving draft tree; it is not an
exact-SHA gate, workspace compilation, or wasm evidence. No earlier result is
relabeled to cover this SHA.

## Independent review

Native sub-agent `/root/review_cryptocom_configuration` inspected committed
31ad96 bytes, excluding working-tree changes. One blocking finding: the six
unsupported comparisons prevent auth witness compilation. No additional
production mismatch found. The review explicitly classified 18 auth and 16
serializer controls as designed, not demonstrated. No readiness verdict.

## Assertion order and controls

EVERY NATIVE ASSERTION BEFORE ANY WIRE ASSERTION: native values, identities,
clock counts, errors, signatures and executed branch counts all precede the
second loop's source output-field-list assertions. Input admission necessarily
precedes execution and is not a wire-output comparison.

There is one compound admission bypass at this SHA:
`signature_without_admission` removes the per-case input/kind/explicit/clockValues
inventory equality loop and the source-count assertion. It also symmetrically
swaps id and nonce in production signature material. The retained independent
native input inventory and signature comparison are designed to fail
`auth_native_signature:0`. This was NOT executed at the audited SHA.
`request_params_projection` is a production identity control, not an admission
bypass; its intended assertion is `auth_native_params_identity`.

The clock repair executes the actual TypeScript getCurrentTimeMs arrow getter,
with time injected into Date.now at offset zero, matching Rust's underlying now
injection. Previously a raw-return stub bypassed the getter's +offset behavior,
including -0 + 0 becoming +0. The input inventory is unchanged. No compiling,
runtime clock-bypass control was demonstrated at this SHA. A subsequent control
must bypass Rust's getter, preserve the raw -0 and fail `auth_native_id` before
this repair has mutation proof. Designed assertions are not measured failures.
