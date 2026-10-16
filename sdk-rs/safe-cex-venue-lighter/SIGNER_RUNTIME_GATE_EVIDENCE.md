# Lighter signer method bodies

Parent actually built on: `bc3b32bce84c67f5feada5f6d8504a6051fecbca`, the requested integration head. Fresh branch: `feat/rust-lighter-signer-runtime`. No unreviewed feature ancestor was added. Implementation and frozen gate SHA: `a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435`.

## Nine pending rows moved

Independent `tomllib` comparison against the parent: **9 pending → ported, 0 → divergent**, unchanged 155 rows in the five owned manifests. Owned `(ported, divergent, pending)` changes **(92, 6, 57) → (101, 6, 48)**. Enumeration stops after these five manifests and nine status changes. Every mapped public method and both supporting struct declarations exist in `src/signer_runtime.rs`; the declaration audit exited 0.

The rows are `TealstreetLighterSignerAdapter.constructor`, `#runWithSubmissionLock`, `initialize`, `createOrder`, `createOrders`, `createGroupedOrders`, `cancelOrder`, `updateLeverage`, and `approveIntegrator`. Signer methods retain the original request/client/crypto handles and errors, serialize submission, reset nonce before classification, wait exactly 150 ms for one retry, reset on retry failure, and release their lock on successful or failed callbacks. Empty batches initialize/check before their length guard; leverage success returns undefined.

The whole-class row stays **pending** with an explicit note. The native struct supplies partial state and these nine constructor/method bodies, but `getAuthToken`, interface/factory and full callable/prototype assembly remain pending. This is an explicit Rc receiver adaptation of the method bodies; raw JavaScript bound-arrow/prototype identity is not claimed. Constructor/runtime support targets are real declarations, not placeholder aliases for missing source symbols.

This slice authors only the owned `lighter.signer.ts.toml`, its native module/export, oracle/test, mutation driver and this sheet. It does not author `lighter.exchange.ts`, SDK source/manifests or shared types/traits. Existing native SDK Promise/error records, deployment and nonce helpers are imported; SDK calls, loader and crypto API are explicit dependencies, with original opaque handles forwarded. No substitute signing or hashing implementation was introduced.

## Gates measured on the frozen SHA

All cargo invocations were sequential, at most two jobs. `CARGO` denotes `/opt/homebrew/opt/rustup/bin/cargo`; `YARN` denotes `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js`. Every native test invocation sets `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| SHA | Command | Observed result |
| --- | --- | --- |
| `a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435` | `CARGO check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --all-targets --jobs 2` | exit 0; Finished dev profile |
| `a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435` | `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_signer_runtime` | exit 0; 4 passed, 0 failed, 0 ignored |
| `a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435` | `YARN parity:extract:check` | exit 0; 7341 symbols across 512 files; would update 0 manifests, 0 added, 0 orphaned |

The writer was run after manifest changes and reported 1 updated manifest, 0 added/orphaned symbols, exit 0, before committing this SHA. Precommit development compilation found one fixture integer/f64 literal mismatch (exit 101); it was corrected before the frozen gates. That compile error is not a discriminating mutation proof. The first passing development fixture was subsequently strengthened with client receiver and constructor-index assertions before freezing.

## Native evidence and its limits

The oracle executes the unmodified complete TypeScript signer module and actual source deployment/domain enum helpers. Controlled SDK constructors/methods, readiness/loader, crypto handle and timer stand in for imported dependencies. Independent floors: **12 initialization + 44 submission + 6 queue/lock = 62 cases**. A fourth native test separately drops the first submission observer while verifying completion/unblocking of its peer. Source queue snapshots stop after 32 Promise turns; native queue draining stops at a floor of 200 callbacks. These are sampled floors, not general scheduler completeness.

Actual native results are compared with decoded source values, preserving UTF-16, BigInt, binary64, arrays and objects. Native constructor account/key values and original options/API identity are asserted immediately. Native constructed client handles are stored separately and signing/reset receivers are asserted by Rc identity before action; crypto/API constructor identities and request handles are checked. Native result/error kind precedes unwrap; generated names/messages, injected error identity, initialization/reset receipts, initialized state, constructors and requests all precede trace assertions. There is no native decode/re-encode comparison.

Cases distinguish initialization already-complete/missing-secret, readiness race with a second failure, matching initial loader rejection, constructor race, deployment variants and changing secret reads; all six exact SDK methods, batch hashes, empty initialization failures, pre-try length getter failure, synchronous signing throw versus rejected Promise, reset/classifier/delay failures, retry failure and retry-reset override; held sends, rejected/throwing lock callbacks and detached observers. Retained reference identity and native FIFO blocking are independent assertions, not merely event strings.

Read-only sub-agent review found two gaps: claiming complete class assembly while auth-token was missing, and fixtures ignoring signing/reset receivers. The class stays pending and direct native receiver assertions now close the latter gap. The reviewer rechecked all nine real targets and five unique mutation anchors, and ran no cargo, tests or mutations.

General JavaScript microtask interleavings, exotic receiver/array/property behavior, raw class reflection, live client replacement during signing and concrete SDK/crypto/loader binding are unproved. Native callbacks specify those imported contracts; this gate proves the caller bodies and sampled values, not cryptographic preimages or live exchange execution. The historical all-zero-fee signing corpus limitation remains in the approval method's own note; this wrapper gate does not retroactively make that corpus independent.

Full workspace compilation, full venue suites, host/wasm clippy, wasm builds, canonical Rust-target checker, CI and live signing/network tests were not run for this slice. The earlier canonical wildcard-import finding is recorded in `BUILDER_APPROVAL_READS_GATE_EVIDENCE.md`; no new canonical pass is claimed here.

## Discriminating compiled controls

`/opt/homebrew/bin/python3 sdk-rs/safe-cex-venue-lighter/scripts/signer-runtime-mutations.py` completed with **exit 0**, frozen SHA `a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435`. Each real source mutation compiled and failed at the named native assertion with exit 101; restoration then passed its exact single test with exit 0 and 1 passed. Compilation errors and trace-only failures cannot satisfy the driver. Enumeration stopped at five controls, with no survivors in this floor.

| Family / mutation | Exact test | Named native assertion | Broken / restored |
| --- | --- | --- | --- |
| Constructor: exchange account/key storage | `signer_initialize` | `signer::native constructor account index` | 101 / 0 |
| Initialization: change matching WASM phrase | `signer_initialize` | `signer::native outcome "init-race":0` | 101 / 0 |
| Shared submissions: 151 ms instead of 150 | `signer_submissions` | `signer::native requests "createOrder-nonce"` | 101 / 0 |
| Lock: await a fresh fulfilled predecessor | `signer_submission_lock` | `signer::native blocked requests "queue-ok"` | 101 / 0 |
| Receiver finding: reset nonce on HTTP client | `signer_submissions` | `signer::native reset receiver "createOrder-nonce"` | 101 / 0 |

Each invocation is `CARGO test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-lighter --jobs 2 --test live_signer_runtime -- <test> --exact`, with the TypeScript environment above. The driver restores source in `finally`, verifies byte identity after all controls, and stores logs/receipts under ignored `sdk-rs/target/lighter-signer-mutations/`. `git diff --exit-code HEAD -- sdk-rs/safe-cex-venue-lighter/src/signer_runtime.rs` also exited 0 after the run. Generated output is not committed; this sheet is authored evidence.

## Merge-down measurements

Fetched and merged integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` into this branch in merge SHA `500634a7667a93a0be0f48dcc822a33fbf7c5a3f`. The only conflict was the Lighter module export tail; resolution preserves both `signer_runtime` and incoming `exchange_reconciliation`. No CLAUDE conflict occurred.

The required whole venue source/test comparison, `git diff --exit-code a5ae0268a2b684e6d80d9f14b95a0c33c2bb7435 500634a7667a93a0be0f48dcc822a33fbf7c5a3f -- sdk-rs/safe-cex-venue-lighter/src sdk-rs/safe-cex-venue-lighter/tests`, exited **1**: incoming reconciliation module/test and `lib.rs` changed. Thus affected gates were rerun; whole-crate preservation is not claimed.

On exact merge SHA `500634a7667a93a0be0f48dcc822a33fbf7c5a3f`, the same scoped check exited **0** (Finished dev profile), the same scoped signer test exited **0** (4 passed, 0 failed, 0 ignored), and extractor check exited **0** with output `Would update 0 manifests; 0 added, 0 orphaned symbols`.

A separate named-input `git diff --exit-code` from the frozen SHA to the merge SHA exited **0**, covering signer runtime, signer helpers, SDK transaction promise/signing/builders, native deployment, the signer test/oracle/mutation driver and owned manifest, all tracked `safe-cex-num` files, and source domain types/signer/deployment types. Enumeration stops at that named set. The five mutation controls remain measured at their frozen SHA, not retrospectively relabeled as executed after merge. They were not repeated after merge.

Independent post-merge `tomllib` and real-declaration audit against integration `9c663a908f40c2955e27b1ce58f97fb5c08fb331` exited **0**: exactly **9 pending → ported**, no divergent moves, owned total 155 unchanged and owned triple **(101, 6, 48)**. Signer alone is **(17, 0, 4)**, total 21. Full Lighter venue is **(229, 19, 88)**, total 336; other-lane progress is not attributed to this slice. Diff against that integration head lists only the owned signer manifest and six venue source/test/script/evidence paths.

No full suites, workspace build, canonical checker, clippy, wasm build, CI rerun, live SDK signing or network test was performed after merge. No PR was created and no branch was merged up into integration.
