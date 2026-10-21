# Remaining family release size profiles

Base: `3a4fbc25e751` (`feat/rust-rewrite`, Bybit size PR #1644 merged).
Test Rust `opt-level = "z"` for the remaining venue families in one PR. Keep
each change only when its public timing/memory gates and complete raw replay
proof pass. Budgets, source behavior and strict comparators remain unchanged.

1. Audit the workspace release profiles and standalone build overrides. Eighteen
   crates still use/inherit level 3: apexomni, asterdex, backpack, bingx, bitget,
   blofin, breakoutprop, cryptocom, grvt, hyperliquid, kraken, kucoin, lighter,
   mexc, okx, polymarket, txflow and woo. Bitrue is a nineteenth case: its crate
   uses z, but standalone family builds override it to 3. Already-z families,
   shared engine/host profiles, LTO and codegen units stay unchanged.
2. Commit this plan before implementation. Test explicit per-package z profiles
   and remove only Bitrue's standalone level-3 override for its experiment.
   Update the nearby release-profile documentation. No runtime mode, parser,
   financial calculation, entropy policy or dependency changes.
3. CI owns release builds, all family/combined size budgets and every public
   CPU/memory gate. No local release builds, full sweeps or timing matrices.
   Use existing native/fmt/Clippy and SDK checks; CARGO_BUILD_JOBS=4 locally.
4. Capture complete source-equivalent baseline and candidate CI raw artifacts.
   Compare all Rust workloads and modes, preserving every identity, request,
   receipt, checkpoint, array order and other field except top-level metrics.
   Require all expected reports/files, stable repeated outputs and at least the
   baseline sample count for every workload/mode; never truncate to available
   samples or add exclusions. Bybit's existing 55-output proof remains separate.
5. Record each family's normal stripped artifact sizes, public performance and
   complete proof. Restore the original profile/override for any family that
   fails a gate; do not relax budgets, timing rules or replay declarations.
   Audit the final configuration against landing to confirm only passing
   families remain. Integrate/trial under lane rules, run final CI, execute a
   fresh separate guard and merge the exact green head. Report the actual pin.

Initial baseline CI: `39652053419`, green head `bcc9574f29c` (all 43 checks).
Its runtime sources and build settings match the base merge; the intervening
landing changed caller documentation only. Refresh baseline provenance if
subsequent source integration changes a workload or build input.

Bybit demonstrated a 143,245-byte saving from z, but no amount is assumed for
other families. Native release builds inherit each retained package setting;
the measured performance evidence here covers WASM. No downstream caller API
or deployment changes are intended.
