// Workspace feature unification hides disabled provider and engine closures.
// Keep those commands intact; only shared changes need the complete matrix.
const checks = [
  {
    "check": "fmt",
    "command": "cargo fmt --all --check"
  },
  {
    "check": "clippy",
    "command": "cargo clippy --locked --workspace --all-targets -- -D warnings && cargo clippy --locked -p safe-cex-crypto --all-targets --features ed25519,es256,secp256k1,stark,schnorr,hmac-sha512 -- -D warnings"
  },
  {
    "check": "test",
    "command": "RUSTFLAGS=-Dwarnings cargo test --locked --workspace && RUSTFLAGS=-Dwarnings cargo test --locked -p safe-cex-crypto --lib signer:: && RUSTFLAGS=-Dwarnings cargo test --locked -p safe-cex-crypto --features ed25519,es256,secp256k1,stark,schnorr,hmac-sha512 --lib signer::"
  },
  {
    "check": "engine-feature-modes-0",
    "command": "python3 scripts/test-http-plan-feature-modes.py --shard 0/4"
  },
  {
    "check": "engine-feature-modes-1",
    "command": "python3 scripts/test-http-plan-feature-modes.py --shard 1/4"
  },
  {
    "check": "engine-feature-modes-2",
    "command": "python3 scripts/test-http-plan-feature-modes.py --shard 2/4"
  },
  {
    "check": "engine-feature-modes-3",
    "command": "python3 scripts/test-http-plan-feature-modes.py --shard 3/4"
  },
  {
    "check": "mutation-barriers",
    "command": "cargo clippy --locked -p safe-cex-engine --all-targets --no-default-features --features http-mutation-barriers,test-support,order-locks,managed-sockets -- -D warnings\nRUSTFLAGS=-Dwarnings cargo test --locked -p safe-cex-engine --no-default-features --features http-mutation-barriers,test-support,order-locks,managed-sockets --test plan_mutation_prepare_barrier --test multi_constraint_pacing\nRUSTFLAGS=-Dwarnings cargo test --locked -p safe-cex-engine --no-default-features --features http-plans,test-support --lib disabled_barrier_refusal\ncargo clippy --locked -p safe-cex-native --no-default-features --features http-mutation-barriers --all-targets -- -D warnings\nRUSTFLAGS=-Dwarnings cargo test --locked -p safe-cex-native --no-default-features --features http-mutation-barriers --lib managed_http\nCARGO_TARGET_DIR=target cargo test --locked --manifest-path hosts/native/tests/barrier-feature-unification/Cargo.toml\nCARGO_TARGET_DIR=target cargo test --locked --manifest-path hosts/native/tests/barrier-feature-unification/Cargo.toml --features host-barriers\nnode --test hosts/wasm/js/hostmanagedhttp.test.mjs hosts/wasm/js/hostmanagedhttp-group.test.mjs\n"
  }
];

export function nativeMatrix(full, crates) {
  return checks.filter(row => full || row.check === 'fmt' || crates.length > 0 && ['clippy', 'test'].includes(row.check));
}
