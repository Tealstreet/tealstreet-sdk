# Crypto.com divergence audit measurements

Fresh integration parent and measured baseline: `d4e20845a85cc4e2711c99648ef7547f8aa806df`, containing merged PR7872 and the PR7854 indexer repair. The branch fast-forwarded from b14e91fbd4 with no conflict; no unrelated unmerged lane ancestor. Crypto.com source, tests and manifests are byte-unchanged from merged PR7872.

The scoped parity-binary build exited0 (locked, jobs2, RUSTFLAGS=-Dwarnings). All three direct maintained canonical binaries now complete indexing: **check1/report1/progress1, exactly5895 diagnostics each**, the known integration baseline. No unsupported item-macro diagnostic remains. Complete diagnostic multisets are equal:2139status/3756invention. [SHA-bound receipts and raw capture hashes](evidence/divergence-audit/baseline-d4e20845a8/canonical.json). The enumeration stops at the last diagnostic recorded in each receipt;5895 counts occurrences, not distinct rows or behavior bugs.

The strict census initially rejected the known report/progress refusal header, giving collection exit1. Its corrected read-only processing removes exactly that wrapper before the maintained classifier; it suppresses no diagnostic. Classifier exits0 after correction. Canonical exits remain1 and no canonical validated report/progress is claimed. Full generated captures are retained locally, not committed.

This step changes no Rust, TypeScript, manifest, status or target. Workspace/venue checks, clippy, wasm execution and runtime mutation controls were not rerun at this merge; their previous measured revisions remain in GATE_EVIDENCE.md.

Backlog reconciliation measured at `30ad96289dad8aa13d0f54160142cd0b51f5de0c`: tomllib119ported/18divergent/0pending over sixfiles/137rows, ending fileindex5 cryptocom.ws-public.ts.toml; no FINDING prefixes. [The seven explicitly expected rows are already ported](evidence/divergence-audit/merged-closure-verification.json), and Crypto source/tests/scripts/reference/manifests match merged PR7872 byte-for-byte (git diff0). This checkpoint moves **zero rows**. Shared integration dependencies/configuration lie outside that scoped identity claim. The next audit freezes the actual18 divergent rows, not the stale20 estimate.

## Completed audit and controls

The historical paragraphs above describe their pinned checkpoints, including the then-zero FINDING prefixes. They are not the final audit classification.

At **`501678668536dcdcf83869592f2a2ffd9c7a7ea4`**, measured sequentially with locked dependencies, jobs2 and RUSTFLAGS=-Dwarnings:

| Named gate | Exit | Scope |
| --- | ---: | --- |
| fmt |0|Crypto.com crate check|
| scoped-check |0|Crypto.com all targets|
| focused-tests |0|api_endpoints, auth, divergence_audit, signing, types:7 tests|
| host-clippy |0|Crypto.com all targets|
| wasm-clippy |0|Crypto.com all targets, wasm32-unknown-unknown|
| extract-check |0|actual Yarn command; output verified, no manifest drift|
| native-public-mutated |101|compiles; AUDIT-NATIVE-PUBLIC-BOOK runtime assertion|
| native-public-restored |0|same one-test target, exact source bytes restored|
| source-public-mutated |101|AUDIT-SOURCE-EXEC / AUDIT-SOURCE-MUTABLE-PUBLIC runtime assertions|
| source-public-restored |0|same one-test target, environment control removed|

[Commands, logs, durations and hashes](evidence/divergence-audit/5016786685/gates.json). [Two controls](evidence/divergence-audit/5016786685/controls.json), stopping index1, have no survivors or compilation-blocked mutations. The source control skips a probe's in-memory table update; it proves probe admission, not a mutation of production TS. The native control changes the actual production lookup with a retained patch. Source restoration was separately verified against HEAD.

[Preflight/helper attempts](evidence/divergence-audit/5016786685/harness-attempts.json) retain four failures/attempts separately: an uncommitted source-helper exposure error, uncommitted scoped compiler preflight, collector expectation of writer wording instead of check wording, and a formatting-sensitive mutation search before any mutation ran. These are not runtime proof. Completed Cargo gates were not rerun merely because a collector stopped after them. No Cargo or Cargo-spawning metadata operation overlapped this audit queue.

At **`e157a6a840eae292b84a9cd22b839ab3e88a8837`**, the review corrections are annotation-only. The standalone source/tests identity command against501 exited0; that is an identity claim, not a relabelled Cargo run. Actual extractor check0 again reports no drift. The manifest writer ran after review note edits and before the commit, exit0, updating0 manifests. [Exact annotation/identity receipts](evidence/divergence-audit/e157a6a840/annotations.json).

At e157, direct maintained **check1/report1/progress1** each reach5895 diagnostics: the full multisets match one another and the d4 integration baseline exactly (added0/removed0). Indexer remains clear. The known baseline includes77 diagnostics naming Crypto.com; this audit does not resolve those mapping/index defects. [Canonical measurements](evidence/divergence-audit/e157a6a840/canonical.json). No global validated parity report is claimed. The direct binaries were built at d4; parity-tool source identity to e157 is0 and binary hashes are retained.

The [tomllib/cohort verification](evidence/divergence-audit/e157a6a840/verification.json) exits0: **119ported/18divergent/0pending**,137 independently expected rows over6files. Eighteen existing notes changed; statuses/layers/targets/file declarations moved0. Frozen cohort enumeration reaches iteration17, CryptocomAccountsResponse. Audit result: **1 supported /17 findings (1behavior,16justification)**. Crypto.com owns those17 newly recorded audit findings; they do not reopen or double-count PR7872's seven closed backlog rows. Production Rust and maintained venue TS identity to d4 exits0.

Not run at501 or e157: full workspace fmt/check/build/host-clippy, full policy wasm build/clippy, actual wasm execution, full venue all-target test execution, full monorepo suites, remote CI or exchange-engine integration. Scoped all-target compilation/clippy and seven focused tests are the claims above. Earlier workspace/wasm execution measurements remain attached to their original PR7872 SHA in GATE_EVIDENCE.md. The inherited **development Cargo-overlap deviation** remains disclosed on PR7959 itself; no new overlap occurred here.

Adversarial review used two local sub-agents only. Pass1 found annotation accuracy issues (signer alternatives, explicit MIXED subcauses/owner, interval path, capture-stage refusal, final Book floor), all corrected in e157. Final retained-evidence review is recorded in the following delivery receipt. There is no human review gate and this branch must not be self-merged.

Final [retention review receipt](evidence/divergence-audit/reviews.json): both sub-agents return bounded PASS at31f8712f387276df44fa664ae07985cf976c08fc plus the final capture-type documentation correction. Each verifies ten retained gate/control log hashes; the interval/DTO reviewer also independently compares all six locally retained baseline/e157 raw canonical multisets. The function reviewer certifies the retained receipts without claiming that full independent recomputation. Both read-only verifications exit0; neither runs Cargo, metadata, tests or mutations. The final commit changes documentation/evidence only. Gates keep their original501/e157 measured SHAs.

## Fleet finding types and behaviour-confidence report

The reporting follow-up begins at36c11ab321b43909a547d7ccb0acba87c28aab67. Read-only historical tomllib verification exits0: ac0f112/20/5→merged PR7872 119/18/0, with all five pending and both original socket FINDING rows now ported. No new status movement is claimed. The two closed original findings account for20→18 in the current divergent cohort.

[The fleet report](evidence/divergence-audit/fleet-finding-types.json) refines the earlier16 justification findings into **13 explanation /3 scheduling**, preserving the separate **1 behaviour**, with **0 documentation /0 already-known** and **1 supported**. Already-known0 means no duplicate previously open finding in this remaining cohort; the three timing mechanisms themselves were documented previously. No new scheduling bug or live scheduler failure is inferred. Source/type inspection exits0 from both sub-agents confirm this classification. The row enumeration remains18, ending iteration17 AccountsResponse.

Behaviour evidence is explicitly separate: the501 native run compares **7 distinct executable rows** directly with the maintained TS implementations, using fixed29-URL,280-params,42-auth and31-interval corpora. Another **11 erased interface rows** have TS compiler-schema, JSON.parse and decoded-native-value evidence (269 positive cases); they are not TS method-body comparisons. The independent seven-row enumeration is not inferred from seven Rust test functions. Request/WS have explicit pre-poll clock probes; signature deferral follows source inspection and ready-poll tests. This is bounded corpus evidence, not a claim that seven rows are fully equivalent. Explanation findings alone establish no behaviour parity, and the mutable-public-table behaviour mismatch stays open.

This follow-up changes documentation and reporting metadata only. Cargo, extractor and canonical gates are not rerun or relabelled; the501 and e157 measurements above remain their exact measured claims. Fresh identity/census and final sub-agent review receipts follow in the reporting verification file.

[Reporting verification and sub-agent review receipt](evidence/divergence-audit/fleet-report-verification.json), measured at793bff09e828ec575f890ba75fdb8eee215ccf63: census/type/independent-row verification0; source/tests/manifest/reference/spec/Cargo identity to36c0; source/tests identity to5010. Both final sub-agent reviews return bounded PASS with read-only checks0. No Cargo/metadata/tests/mutations were run by reviewers. This final retention step adds documentation/evidence only.

Ordered backlog recheck atd81e14ff0a7f1fffa3d572a1df4fd13eda725708 exits0. [The two original socket-class FINDING rows are classified individually as behaviour, code-to-fix, completed by merged PR7872](evidence/divergence-audit/ordered-backlog-checkpoint.json): both formerly required application-supplied venue-member factories; concrete native bindings closed that implementation gap. They are not explanation findings. All5 pending closures remain ported. The original20 divergent rows therefore leave18 in the current audit, with the separately recorded13/3/1/0/0 finding split and1 supported explanation. No new status movement, test run or gate relabelling occurs in this checkpoint. The Bitrue follow-up branches independently from integration and does not inherit the unmerged Crypto.com audit PR.
