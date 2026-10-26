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

## Production artifact inventory

| Consumer | Owned Rust artifact | Source |
| --- | --- | --- |
| Browser SDK and browser worker | One of 28 web family modules | `sdk-rs/hosts/wasm/js/engine.web.mjs`; package `./engine` browser export |
| Node/Bun SDK hosting | Combined Node bindings and wasm | `sdk-rs/hosts/wasm/js/engine.node.mjs`; package `./engine` node export |
| exchange-mcp bundle | Combined Node module copied to `dist/wasm/rust` | `packages/exchange-mcp/tsup.config.ts`, `src/rust-engine.ts` |
| Compiled Bun CLI | Combined web bindings and embedded wasm file | `apps/cli/src/rust-engine.ts`, `scripts/build.sh` |
| Cloudflare Workers/Durable Objects | Deployment-injected module; documented family loader option | `sdk-rs/hosts/wasm/js/workers.mjs` `createEngineObject({loadEngine})` |
| React Native | Host-injected module or native bridge | `packages/safe-cex-react-native/src/rustEngine.ts`; no separate Rust wasm build |

The explicit `./web` and `./node` package exports expose the combined builds
for direct consumers. Both binding layouts share the same stripped wasm bytes.
There is one Rust wasm cdylib in this workspace; crypto and registry code are
linked into it, rather than shipped as separate Rust wasm artifacts. The other
tracked shipped wasm is Poseidon (`packages/poseidon-crypto/src/wasm`), built
with Go, also copied by exchange-mcp. Rust opt-level does not apply to it. This
is a source/build inventory, not a deployed-binary inspection.

## Initial experiment and selection gates

Baseline family evidence: green #1656 CI `39658033928`, source-equivalent to
base `01bfcbbb9d0f` apart from caller documentation. All 32 suites are complete:
438 stable raw Rust outputs, no variance. The subsequent landing push
`39660462768` hit the known Aster signing-clock harness test before replay, so
its incomplete suite is not counted as evidence.

Initial candidate `5e5bd320220`: family CI `39661583419`; combined candidate
`39661578191`; combined baseline `39661495401` on `d61577a43f1` (plan and CI
layout selection only, original wasm settings). The combined module measures
11,026,198 -> 9,795,511 bytes, saving 1,230,687 bytes; full proof remains pending.

WOO's isolated family misses public steady-state CPU: TS 27.83 ms, Rust
29.368 ms, exceeding the unmodified max(1 ms, 5%) allowance. Restore its exact
original build arguments; do not rerun to select around this failure. Combined
WOO is measured independently and initially passes. Other results remain
subject to the complete matrix and final integrated run.

Pin public CI invocations to nine interleaved pairs, the existing small-window
count. This strengthens collection and avoids a seven-pair candidate being
insufficient against a nine-pair adaptive baseline (observed for BloFin). No
sample is omitted, and the performance policy and strict verifier are unchanged.

Final integration order: #1655, #1658, then this PR's exclusive source window.
Refresh source-equivalent baselines after integration and require green final
CI plus the complete raw proof for both layouts before the separate guard.

The broad combined z candidate subsequently misses Bitget and KuCoin Spot
public CPU gates (the original combined profiles pass both). Reject this
configuration; test a narrower combined override of the wasm host alone,
keeping the engine and dependencies at their existing profiles. This is a
changed compiler configuration, not a repeat to select around a timing miss.
The initial 1,230,687-byte saving is therefore not a retained result.

Initial family collection is now complete: all 32 suites, 438 baseline /
442 candidate raw Rust outputs, zero changed hashes and zero unstable groups.
Exactly 31 suites meet the per-workload sample-count proof; BloFin alone has
7 versus 9 performance samples and remains incomplete despite the larger
aggregate count. WOO alone fails timing. The original combined baseline
passes all 32 public suites. Final nine-pair collection must close BloFin
and certify the selected configuration on integrated landing.

## KuCoin Spot throughput restoration after repeated landing misses

The #1661 isolated-family selection is narrowed for KuCoin Spot after three
unrelated CI timing misses. Its venue crate was already level 3; its host,
engine and inherited dependencies still used shared z. `build.sh` now restores
that whole family to its exact pre-#1661 arguments in both `--venue kucoinspot`
and `--families`. KuCoin derivatives keep z. WOO and every other family's
selection, combined/native profiles, `wasm-opt -Oz` and budgets stay unchanged.

The trigger includes landing #1673 (`21a9081c18c`, run `39677179409`: Rust only
9.1% faster than TS against a 10% requirement) and #1677's unrelated Robinhood
PR (0.185% short, retry passed), in addition to the earlier miss. A successful
retry does not erase the tally. AsterDEX remains at one miss and is unchanged.
This restores performance margin rather than repeatedly rerunning a thin gate.

[Restoration plan and proof requirements](RUST_KUCOIN_SPOT_THROUGHPUT.md):
current-source baseline #1679 CI `39679489058` has two workloads, three mode
groups and 11 complete stable Rust outputs, including nine public timing pairs.
Before merging, require the candidate's full matching raw set, unchanged hashes
except metrics, public CPU/memory gates and the fixed artifact budgets. Compare
actual stripped bytes; no local release build or threshold adjustment. This
compiler-only change has no caller API or deployment migration.

## AsterDEX throughput restoration after repeated timing misses

After four owner-counted unrelated public timing misses, restore only the
AsterDEX family to its effective pre-#1656/#1661 compiler settings: host, engine,
Aster crate and dependency defaults at level 3. Preserve Binance's historical
explicit z profile. The family-only venue override leaves combined/native
profiles and all other 27 family argument arrays unchanged. No budget or
timing-policy change. Use a fresh post-#1722/#1726 baseline; the old checkpoint's
evidence is superseded.
[Plan and complete raw/timing evidence](RUST_ASTERDEX_THROUGHPUT.md).

## AsterDEX venue crate at "s" (opt-s follow-up)

After the managed request funnel (#2136, #2148, #2166), the AsterDEX family
carries the engine plan machinery, and its venue crate's level-3 override
cost about 117K. The overseer's slice 3 ruling makes "s" the first lever.
The family keeps size defaults for the host and engine, and its venue crate
moves from 3 to "s". The counterbalanced CPU gate must pass. If it does, the
AsterDEX per-venue budget drops to the CI-measured size plus 4 KiB. Local:
1,444,620 bytes at 3, 1,326,996 at "s", 1,271,168 at "z".
