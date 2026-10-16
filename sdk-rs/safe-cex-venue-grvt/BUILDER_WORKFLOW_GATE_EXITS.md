# GRVT builder workflow gate exits

Branch: `feat/rust-grvt-builder-workflow`.
Actual starting parent: `f0cf136efbc17b93fe52369e71342e620dd925ff`.
Implementation and mutation baseline: `02dcdf2b35bcecddcb2920c44f8a82cc7b175162`.
First merge: `a226f56bc532c83eb35af6d78ef38b7f7a9ab408`, integration parent `4c958de5469ba6ff2ea5ee3be7447cce5abb07c4`.
Second merge: `daa122778abad2207698dc0652420fbeac66d424`, integration parent `85f3b9e596582fce3738807c4d9b7032c5b5a802`.
The previously unmerged public-feed branch was absent from the starting parent; it arrived through integration at the first merge. No separate unreviewed feature branch was merged into this branch.

## Rows moved

Independent Python 3 `tomllib` reads of committed git objects enumerate nine GRVT manifests, with an independent expectation of 164 symbols. Starting parent -> implementation: 65/47/52 ported/divergent/pending -> 73/47/44. Each integration parent -> its merge: 70/48/46 -> 78/48/38. Census at the second merge SHA exited 0 after asserting exactly eight pending->ported, zero pending->divergent:

- authenticateGrvt
- fetchGrvtMainAccountId
- getEthereumProvider
- isBrowserWalletAvailable
- getConnectedWalletAddress
- requestWalletConnection
- checkGrvtBuilderAuthorized
- authorizeGrvtBuilderWithBrowserWallet

These are executable generic Rust workflow bodies in `src/builder_workflow.rs`. The `BuilderHost` binds primitive JS operations, opaque identity-preserving values, fetch/headers/timers, proxy helpers and wallet/ethers operations. Its owned promise contract requires eager prefix execution and continuation ownership after handle drop. A concrete browser or native network adapter is outside this slice. No arbitrary-value restriction is introduced by converting these workflow arguments to existing UTF-8 record codecs.

## Measured gates

All cargo invocations were serial and used `/opt/homebrew/opt/rustup/bin/cargo`, `--manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --jobs 2`.
All source-oracle test invocations set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate | Implementation SHA | First merge SHA |
| --- | --- | --- |
| cargo check, `--all-targets` | 0 | 0 |
| cargo test, `--test builder_workflow`, Node 24.20.0 | 0, eight passed | included below |
| Same target, Node 26.7.0 | 0, eight passed | included below |
| cargo test, `--test builder_workflow --test public_feeds`, Node 24.20.0 | not run | 0, eight + six passed |
| Same two targets, Node 26.7.0 | not run | 0, eight + six passed |
| parity:extract:check | 0 | 0 |

Node 26 commands prepend `PATH=/opt/homebrew/opt/node@26/bin:$PATH`.
Extractor command: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`.
Both recorded extractor checks printed `Would update 0 manifests; 0 added, 0 orphaned symbols.` The writer was run after the manifest edits and printed `Updated 1 manifests; 0 added, 0 orphaned symbols.`, exit 0. Manifest serialization used smol-toml, followed by this writer.

## Discriminating controls

Each control starts from the implementation SHA plus exactly the stated temporary production-source mutation. Each broken invocation compiled and ran one exact named test, exited 101 at the named assertion, then the original source bytes were restored and the same command exited 0 with one passed. Command suffix: `--test builder_workflow <target> -- --exact`. All controls used Node 24.20.0. None survived.

| Target | Temporary change | Named failure | Broken / restored exit |
| --- | --- | --- | --- |
| provider_native | Missing window returns false instead of null | PROVIDER native result native boolean | 101 / 0 |
| available_native | Invert strict null comparison | AVAILABLE native result native boolean | 101 / 0 |
| connected_native | Replace eth_accounts with eth_requestAccounts | BUILDER native eager prefix native string | 101 / 0 |
| request_native | Reject numeric code 4002 instead of 4001 | REQUEST native wallet outcome native string | 101 / 0 |
| authentication_native | Remember-Me true becomes false | HTTP native headers native string | 101 / 0 |
| main_account_native | Valid address becomes null | MAIN native address native null | 101 / 0 |
| check_native | Fee multiplier 10000 becomes 1000 | CHECK native result | 101 / 0 |
| authorization_native | REST spot fee becomes WRONG | HTTP native payload native string | 101 / 0 |

Restoration `git diff --exit-code 02dcdf2b35bcecddcb2920c44f8a82cc7b175162 -- <crate>/src <crate>/tests` exited 0 before merging.

## Coverage floor and review

The actual TypeScript declarations execute in `tests/builder-workflow-source.cjs`, using locked ethers 6.17.0 for address validation and mocked external operations. Independent fixed case counts total 79; enumeration stopped at provider 5/null-window, available 5/null-window, connected 7/null-accounts, request 8/null-error, authentication 12/header-error, main account 9/candidate-error, check 12/alias-mutation, authorization 21/fee-error. This is a floor, not a completeness proof.

Each target asserts decoded native results, own undefined fields, Number bits (NaN classification), native request headers and payloads, and signing arguments before comparing wire strings. Independent request/sign counts and fixed header/domain/message/type-field counts precede the wire checks. Source/native call ordering, eager prefixes, provider identity, proxy alias mutation, module signing-type identity after signer await, and dropped return-handle survival are measured. Abort fetch stays pending before timer firing; the signal change causes rejection. Retained timers are fired after workflow completion and assert original controller identity and independent aborted-controller IDs.

Read-only sub-agent `review_snapshots` approved production and tests within the 79-case floor. Its signing-type singleton finding and preprogrammed-abort fixture finding were corrected before the implementation SHA. Review ran no cargo gates. Synchronous JSON-method throws, provider/signer receiver identity and raw exception identity remain unproved coverage edges; they are not reported as tested.

## Merge survival

The required whole-crate `git diff --exit-code <implementation-SHA> <first-merge-SHA> -- <crate>/src <crate>/tests` exited 1: integration added public-feed source/tests and a module declaration. The sole conflict was `src/lib.rs`; resolution retains both public_feeds and builder_workflow. The three builder files and actual TypeScript builder source were byte-unchanged, scoped diff exit 0. Cargo.lock gained two dependencies for other venues. Scoped check, both changed test targets under both Node majors, and extractor were rerun at the first merge SHA as recorded above.

At the second merge, the whole GRVT source/tests and actual TypeScript builder source comparison against the first merge exited 0. The broader input comparison exited 1 because safe-cex-exchange changed; package.json and the manifest specification also changed. The gates were rerun at `daa122778abad2207698dc0652420fbeac66d424`:

| Gate at second merge SHA | Measured exit and output |
| --- | --- |
| cargo check with `--all-targets` | 0, compile completed |
| cargo test with `--test builder_workflow --test public_feeds`, Node 24.20.0 | 0, eight + six passed |
| Same two targets, Node 26.7.0 | 0, eight + six passed |
| pinned parity:extract:check | 0, Would update 0 manifests; 0 added, 0 orphaned symbols |

The implementation, first merge and second merge SHA references above identify the actual measured trees. This sheet is committed as a separate evidence-only descendant; its addition does not change source, tests, manifests, dependencies or oracle inputs.

## Explicit nonclaims

No workspace compile, full crate or monorepo suite, host clippy, wasm compile/clippy, CI rerun, browser automation, real wallet signature, live HTTP authentication/authorization, or exchange account test was run. The browser/network adapter and deployment are not claimed. The generic workflow mapping and the measured source-oracle cases are the claim.
