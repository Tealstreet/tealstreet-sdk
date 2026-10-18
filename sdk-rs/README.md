# safe-cex Rust SDK

This Cargo workspace scaffolds the Rust port of safe-cex. The TypeScript
[`packages/safe-cex`](../packages/safe-cex) runtime is the maintained parity
reference, not a legacy copy. The crates currently contain documentation only.

See the [port plan](../docs/plans/SAFE_CEX_RUST_PORT_v1.md) for architecture,
phase boundaries, and parity requirements. `sdk-rs/safe-cex-parity/` is the Rust
parity crate. `sdk-rs/parity/` is deliberately not a crate: it is reserved for
parity data and tooling (`manifest/`, `exempt.toml`, and `extract-ts/`).

With rustup installed, the pinned toolchain, components, and target are selected
automatically. From the repository root:

```sh
yarn cargo:fmt          # format Rust sources
yarn cargo:fmt:check    # check formatting without changing sources
yarn cargo:clippy       # lint all targets; warnings remain non-fatal locally
yarn cargo:build
yarn cargo:test
```

Local scripts may update `Cargo.lock` and emit warnings; CI uses `--locked` and
`RUSTFLAGS=-Dwarnings` to require a committed lockfile and warning-free builds.

The standalone wasm gate runs from the workspace:

```sh
cd sdk-rs
cargo build --locked --target wasm32-unknown-unknown -p safe-cex-types -p safe-cex-num -p safe-cex-store
```

[`scripts/live-diff`](scripts/live-diff/README.md) contains the standalone live
TypeScript versus compiled napi account probe. It records exchange HTTPS
responses for identical-input comparisons, runs public reads before authenticated
checks, and supports a small Binance test-account limit/cancel round trip.
Credentials come only from the documented `yarn keys` CLI; public mode needs none.

The wasm gate commits the pure core to compatibility as dependencies land;
empty scaffolds outside that set compile today without a wasm guarantee.
Shared external dependencies are predeclared for later phases; each crate opts
in when it needs them. Publication and license metadata await decision D4.

`layers.json` declares each workspace crate’s layer, isolated build tooling,
and private derivation package names. `node --test sdk-rs/scripts/check-layers.test.mjs`
from the root reads live locked Cargo metadata and rejects unclassified crates,
lateral/upward edges, and skips beyond the adjacent layer. Five existing skips
are explicit exceptions with reasons; removed exceptions cannot linger silently.
Every dependency declaration is checked, including optional, target, dev, build,
and renamed edges. Workspace dependency identity must match the member’s real
manifest directory; name collisions and private derivation paths fail closed.
The SDK’s transitive closure must never reach private venue
derivations. New crates require a deliberate policy placement.
