# WEEX fleet restart audit

The restart brief's pending estimate was stale. The actual parent is integration
`ac0f5fccf24207b26c279e67da2a88f2c72a7aae`. Python 3.11 `tomllib` measured all six
WEEX manifests at that exact revision before any edit:

| Source manifest | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| weex.api.ts | 14 | 0 | 0 |
| weex.exchange.ts | 45 | 3 | 0 |
| weex.types.ts | 26 | 3 | 0 |
| weex.utils.ts | 3 | 23 | 0 |
| weex.ws-private.ts | 12 | 4 | 0 |
| weex.ws-public.ts | 21 | 2 | 0 |
| Total | 121 | 35 | 0 |

Enumeration stops at these six files and 156 rows. **Zero rows moved off pending.**
This is a manifest census, not a new proof of all existing implementation claims.
Existing divergence conventions remain in
`sdk-rs/parity/MANIFEST-SPEC.md#divergence-taxonomy`.

## Annotation-only correction

Five existing CHOSEN notes began with target provenance rather than their bucket:
`WeexExchange`, `WeexExchange.fetchOrders`, `WeexPrivateWebsocket`,
`WeexPublicWebsocket`, and `WeexPublicWebsocket#handleDepth`.
The existing explanation now precedes provenance. The last note also no longer
says its separately mapped class is pending. No status, target, source mapping,
row order, file metadata, Rust implementation or TypeScript implementation changed.
The reconciler preserves all existing annotations; no manifest was regenerated.

## Measured evidence

Frozen annotation SHA: `a3c24759ef3c63af3620d607aa5ceb7467470574`.
Commands use repository-relative paths and tools supplied by the environment.
`YARN` below denotes the configured Yarn entry point invoked through Node.

| Measured SHA | Gate | Observed result |
| --- | --- | --- |
| `25b41576e2336a98efdca314e05862c0f7d3be0f` | Python 3.11 `tomllib` annotation-preservation audit | exit 0; exactly five notes differ; every other parsed field equals the parent; totals remain (121, 35, 0) |
| `a3c24759ef3c63af3620d607aa5ceb7467470574` | Python 3.11 target-file audit | exit 0; 156 nonempty mapped rows, 273 target references, every referenced file exists; declaration resolution is not claimed |
| `a3c24759ef3c63af3620d607aa5ceb7467470574` | `YARN parity:extract:check` | exit 0; would update 0 manifests, 0 added, 0 orphaned symbols |
| `a3c24759ef3c63af3620d607aa5ceb7467470574` | `cargo run --manifest-path sdk-rs/Cargo.toml -p safe-cex-parity --jobs 1 --bin parity-check -- --workspace sdk-rs --manifest sdk-rs/parity/manifest/exchanges/weex` | exit 1; indexing stopped at `safe-cex-webworker/src/chaser_manager.rs`: item macro `thread_local` cannot be indexed without expansion |

The checker did not reach WEEX declaration resolution. Its source and the failing
webworker source are byte-identical to the parent; their comparison exited 0.
The same comparison confirms WEEX `src` and `tests` are unchanged. No separate
baseline execution of the checker is claimed.

After the initial edit and again after the control restoration, `YARN
parity:extract` exited 0 with 0 updated manifests, 0 added and 0 orphaned symbols.
Output text was read. A development audit initially required a colon after every
bucket and incorrectly rejected the existing canonical space-separated spelling;
the instrument was corrected before the frozen audit. It accepts only the five
canonical leading words followed by a colon or whitespace.

For the discriminating control, the actual `WeexExchange` note was temporarily
changed back to a `Split:` prefix. The file-reading audit exited **1** at the named
assertion `weex::canonical bucket prefix WeexExchange`. Restoring the exact bytes
passed with **exit 0**, checking all 35 divergent rows. A subsequent manifest
`git diff --exit-code HEAD` exited 0. Enumeration stopped at this one annotation
control; no native implementation mutation is claimed.

At evidence SHA `8d6ee07ad89bd5123eea06404ab11f6549dcdd5a`, additional controls
used the same restored manifest bytes:

| Deliberate annotation defect | Broken result | Restored result |
| --- | --- | --- |
| Replace the `getHeader` source symbol with an absent audit symbol | extractor check exit 1; named orphan reported, 1 added and 1 orphaned | extractor check exit 0; 0 changes, additions or orphans |
| Replace its first Rust target file with an absent audit path | annotation audit exit 1 at `weex::non-note fields preserved getHeader` | exit 0 |
| Same absent target path | file audit exit 1 at `weex::target file exists getHeader` | exit 0 |

Each probe restored exact bytes in `finally`. These are metadata controls,
not compiled native mutation tests. The writer was run after restoration.

## Merge check and review

The merge-down check against the measured integration parent returned already
up to date. The Rust workflow's literal named-step inventory was unchanged:
**92 before, 92 after**, with identical ordered names. This is an inventory
floor, not a CI execution claim.

An adversarial sub-agent reviewed PR 7846 in a separate clean sparse checkout
at `8d6ee07ad89bd5123eea06404ab11f6549dcdd5a` and recommended **MERGE** with no
blockers. It independently parsed all six files and 156 rows, verified the five
note edits and all unchanged non-note fields, and checked all 273 target
references against 27 existing tracked files. It also confirmed source/checker
preservation and the documented indexing limit. It did not rerun the reported
extractor or mutation controls. The findings were posted on the PR.

## Limits

The existing class rows explicitly retain external callable-adapter assembly;
fixed production body wiring remains an acknowledged omission. The other existing
divergence and bounded-witness claims were not recertified by this census.

No venue compile check, native oracle/test suite, host or wasm clippy, wasm build,
workspace build, full parity pass, CI rerun, live endpoint or signing test was run.
The changed files are annotations and this authored evidence; generated logs stay
outside the tracked patch. No integration merge-up was performed.
