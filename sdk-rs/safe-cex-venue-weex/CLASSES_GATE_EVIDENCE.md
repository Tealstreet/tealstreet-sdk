# WEEX class assembly gate evidence

Parent: `d5d564a0ddc29cc84e1699b30da0726346492599`, the requested integration
head. Fresh branch: `feat/rust-venue-weex-classes`. No unreviewed feature work
was added as an ancestor when this branch was created.

Implementation and native check/test SHA:
`3506bf0baebffab5e365330e58fd24fd27620839`.
Corrected mapping and control SHA:
`e9c5c3ad0b3a819254b5e6390ab117aaaa56c892`.

## Rows and proof boundary

An independent `git show <parent>:<manifest>` / working-tree `tomllib` comparison
measured **0 pending → ported, 3 pending → divergent**: `WeexExchange`,
`WeexPublicWebsocket`, and `WeexPrivateWebsocket`. The 156-row WEEX census moved
from 121 ported / 32 divergent / 3 pending to **121 ported / 35 divergent /
0 pending**. No other symbol status changed.

Each class is CHOSEN divergent: source declarations provide fixed method bodies;
the native factories require complete externally supplied callable adapters to
the maintained heterogeneous member ABIs. These are executable factories, with
construction failure for missing bindings, not fixed production SDK adapters.
The chosen boundary is documented in each row and in `parity/MANIFEST-SPEC.md`.

The new row targets contain 73 references to 31 distinct declarations. A separate
source-declaration audit verified all of them and all 80 member body labels;
canonical parity independently checked mapped target existence and the 229-item
public Rust surface. Labels identify declared provenance, not arbitrary callback
semantics. WEEX has zero exemptions; no gate/exemption rule was weakened.

The class witness floor stops at three class kinds: 45 exchange arrows, 20 public
arrows, public prototype `dispose`, and 14 private arrows. All 79 own arrows are
called with observer adapters to assert original receiver and argument identities;
80 individual missing-binding cases and three first-slot target mismatches reject
before superclass construction. There are two successful native instances of
each class kind to check fresh arrows/collections and shared imported identities.
The source oracle extracts and instantiates actual unmodified TypeScript class
bodies with controlled imported bases. Its own-field floors are 56 / 32 / 24:
53 / 28 / 20 derived fields plus 3 / 4 / 4 controlled base fields. These counts
are independent constants and actual source AST expectations, not native output
counts compared with themselves.

Native Map/Set/Array/object kinds and sizes, selected scalar defaults, explicit undefined
timers, field order, constructor assignments, parent/store aliases, listener
identity, retained callbacks, and once-per-host prototype identity are asserted.
Representative adapters call actual native `is_algo_order`, `public_ping`, and
`private_ping`; ping assertions inspect shared native IDs, disposal, clock and
send records against decoded source values. Public prototype `dispose` has a
receiver-dispatch witness only. No new test compares re-encoded native bytes.
The existing runtime and public-feed files compare decoded native values before
wire/event traces, stopping at their 42- and 71-fixture floors.

## Measured gates

At `3506bf0baebffab5e365330e58fd24fd27620839`:

- `/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2`: **0**.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test classes --test runtime --test public_feed`: **0**, 4 + 10 + 1 named tests passed.
- `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py`: **1**, correctly identifying 13 unmapped `ClassHost` associated declarations. This was a mapping error, not a passing gate. Extractor in that sequential run was not reached.

At `e9c5c3ad0b3a819254b5e6390ab117aaaa56c892`:

- The correction adds exactly the 13 real trait members to the three class rows; `git diff --exit-code 3506bf0baebffab5e365330e58fd24fd27620839 HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests`: **0**. Native test/check input bytes are unchanged; those gates were not rerun merely for the mapping correction.
- `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py`: **0**, 156 rows / 229 public Rust items / zero WEEX exemptions.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: **0** with text `Would update 0 manifests; 0 added, 0 orphaned symbols.` The extractor stops at its recognized 7341-symbol / 512-file floor. The writer `parity:extract` was run after each manifest edit; arrays were derived from real target declarations and serialized programmatically.

## Compiled mutation controls

The driver is `python3.12 sdk-rs/safe-cex-venue-weex/tests/classes-mutations.py`.
Each control runs `cargo test --manifest-path sdk-rs/Cargo.toml -p
safe-cex-venue-weex --jobs 2 --test classes -- <test> --exact` with the compiler
path above, restores original source bytes in `finally`, and reruns that test.
Controls are measured against `e9c5c3ad0b3a819254b5e6390ab117aaaa56c892`.

| Control | Named native assertion | Broken | Restored |
| --- | --- | --- | --- |
| Exchange mode `oneway` → `hedge` | `classes::native exchange mode` / `classes_exchange` | 101 | 0 |
| Public initial subscription ID 1 → 2 | `classes::native socket initial Public` / `classes_public` | 101 | 0 |
| Private initial subscription ID 1 → 2 | `classes::native socket initial Private` / `classes_private` | 101 | 0 |

The first compiler wait was sampled: its active thread waited in dyld `fcntl`
while loading a proc-macro library. This diagnostic is not a test gate. The
named exchange assertion then failed with native `hedge` versus expected
`oneway`, and its restoration passed. The public ID control also failed its named native assertion and passed after restoration.
The private ID control likewise failed its named native assertion and passed after
restoration. Driver exit: **0**. All three controls completed; no survivor was
observed within this three-control floor.

## Review and limits

`/root/weex_review` performed read-only sub-agent review of implementation,
source layouts, bindings, the allocation/prototype corrections and the explicit
mapping correction; no concrete blocker remained. It did not run cargo, tests,
extractor or controls. Its inspection is not a runtime gate.

Not run: workspace compilation, complete crate/monorepo suites, host clippy,
wasm check/clippy, CI or live exchange/network checks. Not proved: fixed
production body adapters across every slot, full native child socket construction
inside the exchange host, arbitrary JavaScript scheduling/heap/GC or future-return
dispatch (the class fixture exercises synchronous member returns). Imported
client rules, constants, base classes, timers and transport remain capabilities.
Zero pending is a symbol census; it is not production SDK integration proof.

## Merge-down and preservation

Merged integration `775a019fe06014b8b858b3f8f6cbdabac552abaf` into the branch,
producing `e9057075f93b6cf11aaacdc37bd6000bbe5b54be`. The only conflict was
`parity/MANIFEST-SPEC.md`; both the WEEX and Orderly sections were kept.
No CLAUDE.md conflict occurred. No additional unreviewed feature ancestor was
introduced outside the fetched integration branch.

`git diff --exit-code e9c5c3ad0b3a819254b5e6390ab117aaaa56c892 HEAD --
sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests
sdk-rs/parity/manifest/exchanges/weex`: **0**. A second diff of imported WEEX /
shared TypeScript sources, Rust dependencies, Cargo.toml, canonical parity code
and extractor code also returned **0**. All mutation source bytes were separately
verified restored against the working tree before the evidence commit.

Independent Cargo.lock entry comparison found only `safe-cex-venue-woo` changed
(adding its futures-util dependency); it is outside the conservative 408-name
WEEX dependency closure. WEEX scoped exemptions remain zero. Native check,
scoped tests, canonical WEEX parity and the three controls were not rerun after
merge: their tested own-code, dependency and scoped manifest inputs are preserved.
At the merge SHA, the global extractor was rerun: **0**, with text
`Would update 0 manifests; 0 added, 0 orphaned symbols.` Its recognized floor
remains 7341 symbols / 512 source files. Other venue manifest changes are thus
covered by that measured extractor run, not by the earlier native tests.

Integration-relative tomllib census at the merge SHA independently confirms
**0 pending → ported, 3 pending → divergent**, with the same 156 rows and
121 ported / 35 divergent / 0 pending. This compares against the fetched
integration head, not a stale merge base.
