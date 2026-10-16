# WOO V3 order-book recovery gates

Branch feat/woo-v3-orderbook-recovery started fresh from **1cc4fcce5b9dc46a608a26cfd003e66f71b703cb**.
Actual built/gated source, tests and manifest SHA: **ec142eeb4d0fb36e9a6524894c02dba1944164b7**.
The initial implementation fcc7388f440af43e0cd139f0d0ae4f721b3b3e83 and metadata fix are the only new
source ancestors; no unrelated unreviewed lane branch was used.
Integration was fetched and merged down before delivery from **ef054ba0b8e9038241576f44c09f652348f2f4bb** at
**b5586f4215714f98ca6ee398a0c91e8f03fe441e**. Source/test/script/manifest survival comparison with the gate SHA exited **0**.
The slice and checked dependency inputs stayed byte-identical, so the recorded gate exits stand for this unchanged slice.
The evidence is added by a later documentation-only commit; no source gate is claimed as executed at
that documentation/merge SHA. Post-merge measurements are separately SHA-bound in the JSON.
The post-merge extractor and tomllib census both exited 0 at the merge SHA; extractor output remains
7341 / 512, Would update 0 / 0 added / 0 orphaned, and census still reports 4 ported / 19 divergent moves
against the actual integration parent. Those fresh measurements cover the changed global manifests.

## Completed gates at ec142eeb4d0fb36e9a6524894c02dba1944164b7

| Gate | Exit and observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo --all-targets --jobs 2 --offline --locked | 0; Finished dev profile |
| cargo test with the same package/all-targets/jobs/offline/locked scope | 0; 39 targets / 104 passed / 0 failed / 0 ignored |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols / 512 files, Would update 0 manifests / 0 added / 0 orphaned |
| python3.14 scripts/ws-topics-targets.py | 0; scoped invention/status audit: 534 rows / 310 public Rust items |
| rustfmt --check --edition 2024 src/v3_orderbook_recovery.rs tests/orderbook_recovery.rs | 0 |
| node scripts/orderbook-recovery-oracle.cjs | 0; whole actual source modules/dependencies executed |
| python3.14 scripts/orderbook-recovery-census.py 1cc4fcce5b | 0; 23 independently expected pending rows moved |
| python3.14 scripts/orderbook-recovery-controls.py | 0; six compiling named failures and six restored passes |
| git diff --exit-code after controls | 0; exact tracked source restoration |

Cargo used /opt/homebrew/opt/rustup/bin/cargo, one invocation at a time, jobs 2. Oracle tests and controls
used SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Extractor output text
was inspected. parity:extract was the manifest writer after each manifest edit; rust_files was derived
from real rs targets. The target checker scopes 22 WOO/journal manifests and indexes sibling dependencies;
it is not a full-workspace audit. Commands, SHA anchors, initial and final exits, call sites and named
failure excerpts are retained in ORDERBOOK_RECOVERY_GATES.json.

## Initial failed audit and correction

At fcc7388f440af43e0cd139f0d0ae4f721b3b3e83, scoped check/extractor/fmt/census/oracle exited 0, but the target
audit exited **1**: all 23 mappings lacked nonempty layer, and three supporting anonymous-return types
needed their full target paths in the notes. ec142eeb4d0fb36e9a6524894c02dba1944164b7 fills adapted layers
and names the exact supporting targets. Production and test bytes did not change. The audit was rerun
and passed; no exemption or gate was weakened. Initial receipts/log paths are unique to their SHA and
were not overwritten. No test/control pass is attributed to the initial implementation SHA.

## Decoded native assertions and finite floors

The oracle loads the actual recovery coordinator, public REST mapper, sequenced-book utility,
price-update utility and order-book sorting/total utility. No book or sequence behavior is stubbed.
Source numbers are tagged before JSON projection, retaining signed zero/infinity input lexemes.
Native assertions inspect decoded level pairs, lexical wire strings, normalized symbol/timestamps,
exact fingerprint, result variants, optional update timestamp, counts, reasons, flags, phase, request
tokens and every publication price/amount/cumulative total with numeric bit comparisons. Native
assertions precede any wire comparison; no Rust decode/re-encode equality occurs in the new target.

Fixed walks stop at record case 14, timestamp case 14, numeric level case 24, side case 6, update case 12,
snapshot side 2, constructor case 10 and recovery scenario 25. Each scenario also has an independent
literal step count: [3, 5, 4, 4, 4, 3, 3, 4, 4, 6, 4, 5, 6, 7, 6, 8, 7, 3, 4, 8, 3, 3, 6, 503, 4]. The walk stops after **617** steps; queue-ceiling
scenario stops at 503 steps (501 pushes plus request/snapshot). Token exhaustion stops at two source
calls (maximum-safe accepted then exhausted); the native unit also verifies reset retains exhaustion.
Publication ownership uses two independent source publication expectations.

These floors exercise direct and buffered joins, stale updates, exact lexical duplicates/conflicts,
out-of-order buffers, consumed/replaced/stale/malformed tokens, invalid snapshot/delta recovery,
overflow-before-invalid ordering, gap recovery, depth 200/201, queue 500/501, numeric price collision,
zero-quantity removal, REST radix/exponent strings and decimal underflow/overflow.
Multiply-invalid helper fixtures compare the first Error message: symbol before predecessor, predecessor
before timestamp, timestamp ordering before sides, and asks before bids. Source frozen results/publications/levels and mutable independent publication side
arrays are checked. Native results and publications are inspected directly after retained input decoding.
Arbitrary injected JavaScript-private queue state and its otherwise unreachable replay duplicate/stale
branches are not enumerated. No exhaustive runtime/branch/mutation coverage is claimed.

## One compiling control per new family

| Family | Named failed native assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- |
| numeric-level | RECOVERY native level price | 101 | 0 |
| normalization | RECOVERY native price | 101 | 0 |
| request-token | RECOVERY native request token | 101 | 0 |
| buffer | RECOVERY native buffered count | 101 | 0 |
| sequence-join | RECOVERY native result variant joined | 101 | 0 |
| publication | RECOVERY native snapshot provenance | 101 | 0 |

Each control is an explicit production overlay on ec142eeb4d0fb36e9a6524894c02dba1944164b7: swap decoded price/quantity; take asks from
bids; advance request nonce by two; add one to buffered count; use update sequence as its own predecessor;
use live time as snapshot provenance. The named assertions failed after compilation, then the exact
saved bytes were restored and all three orderbook_recovery integration tests passed. Stopped after
mutation iteration **6**, zero survivors in that floor; no additional matrix is claimed.

The native review sub-agent found no production mismatch and verified the 23 real primary targets,
three supporting return types, per-row notes and six unique control anchors. It found a trace-count
gap: all 25 scenario names were pinned but only the queue scenario pinned its operation count. Fixed
independent per-scenario counts were added to both oracle and native walk before the measured gates.
The reviewer ran no cargo/control/extractor/audit gate; none is attributed to it.

## Independent census and adaptations

Tomllib independently reads 22 WOO/journal manifests and asserts the known 534-row inventory plus a
separately enumerated 23-symbol expectation. **23 pending rows moved: 4 ported / 19 divergent**.
Before: 103 ported / 122 divergent / 309 pending. After: **107 / 141 / 286**.
All 23 notes are distinct and name their own main target; supporting anonymous normalized return types
are also explicitly mapped. Ported: exact ceiling 500, phase getter, sequencer application and reset.
Every divergent row has its own canonical CHOSEN explanation rather than a reused family note.

Recovered books group source flattened fields into the existing owned OrderBook; source frozen
Readonly result records become native variants and absence of publication is represented by variant.
Ignored branches share an optional time; reason Strings and recovery bools permit wider direct native
construction. Counters use bounded native indices. Frozen numeric/wire arrays become owned pairs/vectors.
Retained dense JSON excludes executable/accessor/prototype/sparse/lone-surrogate inputs. Result messages
preserve source text rather than Error identity. Class state/book ownership is hidden; native mutex
ownership replaces source private object references. Exact sequence/nonce arithmetic remains f64 where
the source uses Number. REST decoding captures retained inputs into validated native snapshots before
using the existing public mapper; partial snapshot errors become invalid-snapshot as in the source catch.
No shared type/trait changed, no live transport is registered, and no HOW row moved.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW
tests, host clippy, wasm clippy, wasm build, doctests or CI reruns. No exits are claimed for these.

Final delivery refresh: integration 0f7fd2ce92e79d0c38e87ea5dd15b21927ea8c1a merged at 26bdd39308e0c68d0138d745b2eb3dbe686ba050.
The same source/test/script/manifest comparison again exited 0 and listed dependency inputs remained
unchanged. The source gate exits above still stand; this additional receipt is in final_delivery_refresh
in the JSON. No new cargo/test/mutation run is claimed at this merge SHA.
Final-refresh extractor/census each exited 0 at 26bdd39308e0c68d0138d745b2eb3dbe686ba050;
output again reports no manifest drift and the same 23 moves against 0f7fd2ce92.
