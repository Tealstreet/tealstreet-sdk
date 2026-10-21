# Shipped Rust wasm artifact size experiment

Base: `01bfcbbb9d0f` (landing after #1656). Native profiles stay exactly as
they are at this base; use only wasm build-command overrides.

1. Trace the production loaders and effective build flags. Browser/worker
   `engine/web` loads 28 isolated families. Node `engine` loads the combined
   module. exchange-mcp copies the combined Node package beside its bundle;
   inspect the standalone CLI, hosted SDK and React Native injection paths too.
   Workers accept an injected deployment loader, rather than owning another
   Rust binary. Poseidon crypto is Go wasm, outside Rust opt-level settings.
2. Commit this plan first. Add an explicit dispatch-only combined replay choice
   to the existing CI matrix so the same unmodified strict gates run against
   the actual combined artifact before family builds can overwrite it. Keep
   family CI as the default. Never label a combined module as a family-only
   CI reuse package. Collect source-equivalent baselines for both layouts.
3. Experiment with z only through `sdk-rs/harness/build.sh` wasm command
   overrides: shared host/engine and remaining inherited level-3 dependencies,
   preserving explicit venue settings initially. Existing Bitget/Kucoin Spot
   throughput decisions and Paradex overrides are inputs, not assumed winners
   under a different artifact. No source algorithm, dependency, budget,
   comparator, native-profile or release-automation changes.
4. CI owns all release builds and timing sweeps. Require every public CPU and
   retained-memory gate, all fixed wasm budgets, existing native/SDK tests, and
   complete raw output proof for each affected layout/family. Compare all
   reports, workloads, modes and samples against the matching baseline; only
   top-level metrics may differ. Preserve all identifiers, requests, receipts,
   checkpoints and array order. Require at least baseline samples per group,
   stable repeated outputs and all expected files; no missing-output waiver.
5. Keep z only for artifacts passing both timing and complete raw proof; restore
   the prior wasm build arguments for failures. Report the actual production
   artifact mapping, sizes, exact proof counts and any retained level-3 cases
   in the PR. Audit unchanged native settings and every family build argument.
   Integrate relevant landing changes, obtain final green CI under lane rules,
   run a fresh separate guard, merge the exact green head, and report the pin.
