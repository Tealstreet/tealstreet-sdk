# Remaining family release size profiles

Base: `3a4fbc25e751` (`feat/rust-rewrite`, Bybit size PR #1644 merged).
Test Rust `opt-level = "z"` for the remaining venue families in one PR. Keep
each change only when its public timing/memory gates and complete raw replay
proof pass. Budgets, source behavior and strict comparators remain unchanged.

1. Audit the workspace release profiles and standalone build overrides. Eighteen
   crates still use/inherit level 3: apexomni, asterdex, backpack, bingx, bitget,
   blofin, breakoutprop, cryptocom, grvt, hyperliquid, kraken, kucoin, lighter,
   mexc, okx, polymarket, txflow and woo. Bitrue is a nineteenth crate/override case: its crate
   uses z, but standalone family builds override it to 3. Kucoin and Kucoin
   Spot share a crate but are separate standalone families: 20 family cases
   across 24 replay jobs in total. Already-z families,
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

## Initial CI measurements and selection

Baseline CI `39652053419`; candidate CI `39655024681`, head `3c468940797`.
All 24 affected jobs have complete, stable raw proof: 274 baseline outputs
and 276 candidate outputs, with zero changed workload/mode hashes. BloFin
contributes the two additional adaptive performance samples; none are omitted.
Only top-level metrics are excluded. Existing declared correctness failures
remain unchanged; the following timing results use the repository's unmodified
performance policy and validate all public samples.

Exactly 18 of 20 standalone families are selected; Bitget and Kucoin Spot are
rejected on steady-state CPU. Sixteen crate overrides remain at z. Kucoin's
crate default stays at level 3 for native/combined/Spot builds, while its
passing derivatives-only standalone family uses z through the existing
`build.sh` override mechanism. Bitrue's standalone level-3 override is removed.
Shared engine/host profiles and every budget remain unchanged.

| Family | Baseline bytes | Tested z bytes | Saving | Raw outputs baseline / z | Timing | Selection |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| apexomni | 1,108,646 | 1,065,729 | 42,917 | 11 / 11 | pass | keep z |
| asterdex | 1,154,778 | 1,089,669 | 65,109 | 12 / 12 | pass | keep z |
| backpack | 952,484 | 912,881 | 39,603 | 9 / 9 | pass | keep z |
| bingx | 1,037,059 | 971,402 | 65,657 | 11 / 11 | pass | keep z |
| bitget | 1,098,848 | 1,017,111 | 81,737 | 21 / 21 | fail | keep original 3 |
| bitrue | 978,340 | 901,197 | 77,143 | 11 / 11 | pass | keep z |
| blofin | 1,005,193 | 921,008 | 84,185 | 9 / 11 | pass | keep z |
| breakoutprop | 762,588 | 745,038 | 17,550 | 11 / 11 | pass | keep z |
| cryptocom | 842,709 | 806,238 | 36,471 | 11 / 11 | pass | keep z |
| grvt | 1,124,405 | 1,023,950 | 100,455 | 11 / 11 | pass | keep z |
| hyperliquid | 1,191,783 | 1,097,555 | 94,228 | 23 / 23 | pass | keep z |
| kraken | 1,252,369 | 1,109,286 | 143,083 | 18 / 18 | pass | keep z |
| kucoin | 906,895 | 853,264 | 53,631 | 9 / 9 | pass | standalone z; default 3 |
| kucoinspot | 787,135 | 756,948 | 30,187 | 9 / 9 | fail | keep original 3 |
| lighter | 1,175,894 | 1,076,000 | 99,894 | 20 / 20 | pass | keep z |
| mexc | 1,017,180 | 948,615 | 68,565 | 9 / 9 | pass | keep z |
| okx | 980,450 | 899,946 | 80,504 | 36 / 36 | pass | keep z |
| polymarket | 953,774 | 901,808 | 51,966 | 11 / 11 | pass | keep z |
| txflow | 1,075,862 | 1,021,577 | 54,285 | 11 / 11 | pass | keep z |
| woo | 1,016,192 | 954,143 | 62,049 | 11 / 11 | pass | keep z |

Bitget steady-state medians: TS 396.855 ms, z 382.266 ms; required z <=
357.1695 ms. Kucoin Spot: TS 169.955 ms, z 158.363 ms; required z <=
152.9595 ms. No retries are used to select around these measured failures.

Final selection still requires current-landing integration and a fresh green
CI run, including complete source-equivalent raw proof for each retained
family and all fixed family/combined size gates. The overseer's merge order
is #1653, then #1650, then this PR's final integrated CI window.
