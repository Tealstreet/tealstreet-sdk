# safe-cex parity manifest

One TOML file in `manifest/` corresponds to each eligible TypeScript source file
under `packages/safe-cex/src/`, keeping the `.ts.toml` suffix. Rows map TypeScript
symbols to Rust items. See [the parity plan §5](../../docs/plans/SAFE_CEX_RUST_PORT_v1.md#5-the-parity-system)
and the shared [safe-cex guidance](../../packages/safe-cex/CLAUDE.md).

[`MANIFEST-SPEC.md`](MANIFEST-SPEC.md) is the load-bearing contract between the
TS extractor and the Rust validator; its per-file layout supersedes the plan's
original single-file example. Never regenerate over agent annotations or silently
delete orphan rows.

That file used to live outside the repository, in an untracked overseer
directory, and this README linked to it there — so a committed document cited a
path that existed on one machine. The vocabulary it declares (row statuses, the
divergence buckets, the `ts-bug:` and `ref-bug:` markers) is only usable if it
ships with the manifest it governs.

Root commands (wired by the Rust parity package):

```sh
yarn parity:extract        # reconcile TS symbols into the manifest
yarn parity:extract:check  # coverage gate; never writes
yarn parity:check          # Rust invention and status gates
yarn parity:report         # ported/total and divergence by file/directory/crate
```

The extractor's entrypoint is `node --import tsx sdk-rs/parity/extract-ts/src/cli.ts`
from the repo root, with optional `--check`. The `@tealstreet/parity-extract-ts`
workspace uses the root install and lockfile, TypeScript 6, and `tsx`.
Package-local commands are `yarn extract`, `yarn test`, `yarn typecheck`, and
`yarn lint`; each extraction prints counts by kind and top-level source directory.

Callable injection slots with package-local types (for example
`VirtualClock#serverTimeSource` and `CrossTradeManager.getExchange`) remain rows.
Their port annotations must account for the injection seam even when the Rust
implementation places its behavior elsewhere.
