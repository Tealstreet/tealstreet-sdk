> Historical evidence: the test-only V3 comparison/canonical bodies, operation holders and their oracle commands are retired. The live legacy operation shape is retained; see CLAUDE.md.

WOO canonicalizer gate evidence

Nine pending rows moved to ported; none moved to divergent. Mapping and body
contract: sdk-rs/parity/MANIFEST-SPEC.md#woo-public-canonicalizer-bodies.

Measured SHA-bound commands, exits, native test summaries, mutation failures,
restored passes, and merge input differences are in PUBLIC_CANONICAL_GATE_RESULTS.json.
Native gate SHA: f2989eceffbdff23f6b4c484ccf553dcedfbfd81. At that SHA,
scoped all-targets check, 22 selected tests, source oracle, census, extractor
writer/check and four-control driver each exited 0. Controls stopped at four:
each compiled mutation failed 101 at its named native assertion and restored 0;
no survivors. No exhaustive mutation or complete host-assembly claim.

At merge cb753a87fb86ef3a6a3636c696670940ab2db2cd, crate src/tests diff was 1
from imported fill recovery; owned-unit input diff was 0. Fresh scoped check,
30 affected tests, census and extractor writer/check exited 0. Assigned triple
was 13/18/19; WOO 186/298/48 (ported/divergent/pending). Other lane movements
are separate from this slice's nine. Later restart gates are appended to JSON.

Sub-agent context_review identified callback retention and native field/length
assertion gaps; both were corrected before the native gate SHA. Reviewer ran
no gates. Preflight was diagnostic only. Local paths in historical excerpts are
redacted for public-mirror publication; exits remain attached to measured SHAs.

Not run for these recorded gates: full WOO suite, workspace checks/tests,
host/WASM clippy, WASM tests, TypeScript typecheck, live transport or CI.

Restart gate SHA: 4098b22fd0c8f2ac511b8497a6de6552cca73ab4, after merging
integration ac0f5fccf24207b26c279e67da2a88f2c72a7aae. Fresh isolated source
oracle, census, extractor writer/check, scoped all-targets check and 31 selected
tests each exited 0. Selected targets and output summaries are in the JSON.
Isolation stopped after four actual child probes: positive 0, missing compiler 1,
source fundingRate mutation 1 at its named assertion, and a no-op child 0 rejected
by the mutation verifier. The source oracle independently expects 50 property
reads across five projectors. The three owned string unions failed their native
rejection assertions at 2ec6734ed810c1e558123b26489f01b61ae3f46c (101),
then passed at 50d8bbeb6bc7cf24a05878394bc618d8d9b3a2ba (0, three tests).
Assigned triple remains 13/18/19; all WOO is 195/308/29. This branch moves
9 pending->ported and 0 pending->divergent, compared to its actual original parent.
No additional omitted gate listed above was run during restart.

PR 7852 adversarial sub-agent review found the control runner assumed an
untracked artifact directory existed. ca486c4461 creates it before any source
mutation; Python syntax compilation exited 0. Reviewer reports no remaining
blockers at that SHA and no private paths in the reviewed diff. Reviewer ran
no cargo, tests, extractor, clippy, WASM or CI. Final merge was already up to
date; own crate src/tests diff from 4098b22fd0 to ca486c4461 exited 0.
The directory fix changes runner setup only; the four compiled controls were
not rerun for that fix.

Merge reconciliation with sprint batch `dcb88b8e64efc8da8ed831388c9adaec2609b77f`
uses its single CanonicalOps/CanonicalModule implementation and exact batch row
annotations, including downstream snapshot diagnostics. PR 7852's six native
semantics groups are adapted to that implementation in public_canonical_semantics,
with the original source probe in public-canonical-semantics.cjs. The batch oracle
keeps its stdin case protocol. The isolation probes target the semantics probe,
and the four mutation overlays target the integrated bodies. The historical
nine-row-only census runner is removed because the batch includes additional
completed rows; prior receipts above describe their original SHAs only.
