# WOO v3 regular-write gate evidence

Fresh integration base and actual built-on parent: `600bbd302bdd09d20f938650e360385e68f55cba`.
Source gates: `24407a9ab0238c3f1476433f6bb9ae1ae9617d43` on `feat/woo-v3-regular-write-contracts`. Later evidence-only commits do not relabel these exits.
No unrelated unreviewed feature branch was used as an ancestor.

**32 rows moved: 1 pending -> ported, 31 -> divergent**. Tomllib reads a fixed 32-symbol expectation across 22 WOO/journal manifests / 534 rows; distinct notes32 and real primary/supporting targets41. Before: {'divergent': 211, 'ported': 122, 'pending': 201}; after: {'divergent': 242, 'ported': 123, 'pending': 169}. The regular-write file has zero pending rows; this is a census, not full WOO readiness.

## Gates at the source SHA

| Gate | Exit |
| --- | --- |
| check | 0 |
| tests | 0 |
| extractor | 0 |
| targets | 0 |
| census | 0 |
| oracle | 0 |
| fmt | 0 |

Full scoped WOO tests: 43 reported targets, 127 passed, 0 failed, 0 ignored. ONE Cargo process at a time; check/tests/controls use --jobs 2 --offline --locked. Oracle uses SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript. Extractor output inspected for zero updates/added/orphaned symbols; scoped target audit checks all534 WOO/journal rows (446 public Rust items), not the workspace.

## Native discrimination

Every result assertion inspects decoded native fields/variants/f64 bits before any wire claim. No decode/re-encode equality is used. Six compiling controls, stopped at iteration6; no survivors:

| Family / exact test | Mutation | Named native assertion | Broken / restored exits |
| --- | --- | --- | --- |
| declarations / native_declarations | symmetric place declaration price/quantity label swap | REGULAR-WRITES native declaration quantity | 101 / 0 |
| validators / native_validators | invert fractional decimal comparison | REGULAR-WRITES native decimal comparison | 101 / 0 |
| place / native_place_builders | invert hedge reduceOnly side branch | REGULAR-WRITES native captured string | 101 / 0 |
| amend / native_amend_builders | invert target-match amend blocker | REGULAR-WRITES native blocked variant | 101 / 0 |
| cancel / native_cancel_builders | pending cancel uses singular order endpoint | REGULAR-WRITES native request endpoint | 101 / 0 |
| responses / native_response_decoders | gateway concrete error name changed | REGULAR-WRITES native gateway name | 101 / 0 |

Each restored exact test reports1passed/0failed/5filtered. Original source bytes are restored in finally after every mutation. Full commands, call sites and complete logs are retained in the companion JSON.

## Enumeration floors and reviewed boundaries

Source fixture floors, stopping at these counts: `{'validators': 100, 'places': 31, 'amends': 22, 'cancels': 8, 'placeResponses': 12, 'successes': 10, 'mutations': 23, 'declarations': 9, 'declarationRefusals': 8, 'timeInForce': 4}`. Runtime cases206, plus declarations9, present-null/literal refusals8, TIF variants4 and concrete constructor/metadata native assertions. AST pins all five Readonly record declarations, exclusive target union, both ready/blocker declaration branches and exact TIF spellings; source metadata key inventory is fixed at7. These are tested floors, not arbitrary JavaScript completeness.

Type declaration numbers retain f64 (including signed zero and non-finite inputs) before Serde buffering; runtime IDs/timestamps apply their source safe-integer minima. Decimal strings compare lexically, including values beyond f64 precision. Named first-error/blocker comparisons exercise validation order (including envelope data before timestamp and failure code before message), rather than just error presence.

CHOSEN adaptations are justified per row: captured owned records exclude getters/prototypes/sparse arrays/lone UTF-16 surrogates and source frozen or alias identity; targets are an exclusive native ID enum; error gateway trees are deep-owned rather than shallow references; typed boolean/tag guards reject off-signature values. AmendBuildResult is an operation-only native enum, not arbitrary declaration ingress. Public identityEvidence String construction is wider than its source literal (deserialization/decoder enforce the literal). Request bodies/queries remain captured values. No live route, shared type/trait, HOW or second-lane legacy/exchange file changed.

Read-only sub-agent review ran no gates/edits. Its leading-zero helper, stored empty-string, metadata inventory/classification and supporting-target findings were corrected before the source commit; six named control anchors were reviewed and then measured separately.

## Earlier diagnostics

An uncommitted draft scoped check based on600bbd302b exited101 for four test String-versus-&Value type errors; it is not a gate for the clean base or final source SHA. The draft changed while that check was active, so no exact source attribution is claimed for it. A draft target audit exited1 for missing supporting mappings/names. The initial writer exited1 on invalid layer direct; corrected to adapted, later writers exited0, final text reports zero updates/added/orphans. Fresh source-SHA gates above supersede these, without relabeling their exits.

Not run: workspace build/tests, full-workspace parity audit, monorepo suites/typecheck, dedicated HOW tests, host clippy, wasm clippy, wasm build, doctests, CI reruns, live/credentialed demo lifecycle. No exits claimed for them.

After all six controls the scoped source/test/script/manifest restoration diff exited0 at the source SHA. The control driver exited0. A read-only compiler-delay sample exited255 because the compiler process had already exited; no stack was captured and this diagnostic is not a gate.

## Completed merge-down and inherited finding

Actual merge `e5418e43daf2397ba27472e54362d46547d8d68b` includes integration parent
`775a019fe06014b8b858b3f8f6cbdabac552abaf`. Fetch exited0. The initial merge command exited1
for a conflict in the shared lib.rs append; retaining both module exports and committing the
resolution exited0. Own regular-write source/tests/scripts/manifest survival diff exited0.
The broader crate src/tests diff exited1 because integration added second-lane legacy_metadata;
dependency-input diff exited1 for Cargo.lock and Woo Cargo.toml (futures-util addition).
No shared type/trait or second-lane implementation was authored/edited by this slice.

Fresh extractor exited0 (output inspected: zero updates/added/orphans); tomllib census exited0,
against the actual merged parent: **32 moved, 1 ported/31 divergent; pending191 ->159**
(final133ported/242divergent/159pending). The earlier201 ->169 census is retained at the fresh base,
and does not claim those additional10integration rows as this slice's work.

The optional post-merge whole-WOO target audit exited**1** on inherited legacy_metadata mappings
and public helper symbols. First call site: WOOXExchange#fetchAccountInfo ->
safe-cex-venue-woo/src/legacy_metadata.rs::fetch_account_info. That file is absent from the
woo.exchange manifest rust_files, its method rename notes omit exact qualified targets, and host
helpers lack mappings/exemptions in this audit. Every diagnostic is in legacy_metadata; full output
is retained in JSON. Those are outside the user's hard v3 partition, so they remain untouched and
are reported as an inherited finding. The original own-slice target audit exit0 remains bound to24407a9ab0;
there is no claim the post-merge whole-WOO target audit passed, and no gate was weakened.

Post-merge scoped all-target check exited**0** at e5418e43da. Affected regular-write and inherited
legacy-metadata tests exited**0** at that same merge: **2 targets / 13 passed / 0 failed / 0 ignored**
(7legacy +6regular; independently named targets). Full127-test WOO success is attributed to24407a9ab0
only; no full-suite rerun after integration merge is claimed. Runtime exits are measured separately
from the read-only evidence review, which ran no gates or edits and found no new own-slice blocker.
That review confirmed all65target-audit diagnostics are inherited legacy_metadata findings.


## Second merge: final measured code state

Final measured code merge `23966253ed0776b4330f9ea7b53f18ff53088c9c` has actual integration parent
`9bcfd1e1c76d9b330930fe7bf9a0faa362a97c73` (read from the merge commit, not a mutable shared remote ref).
Fetch and automatic merge commands exited0. Relative to the previously checked e5418e43da,
all listed dependency inputs (including Cargo.lock) are byte-identical (diff0), while the broader
crate source/tests diff1 contains the second lane's new legacy_socket_control unit and lib export.
Own source/test/script/manifest diff against24407a9ab0 remains0. Both legacy implementations,
tests and three off-partition manifests match this integration parent exactly (diff0).

At23966253ed: scoped all-target check**0**, affected tests**0** (**2targets/13passed/0failed/0ignored**,
7legacy_socket_control +6regular_writes), extractor**0** (zero updates/added/orphans), tomllib census**0**:
**32moved,1ported/31divergent; pending183 ->151**, final141ported/242divergent/151pending across534rows.
The earlier191 ->159 and201 ->169 snapshots retain their respective parents and are not current counts.

The whole-WOO target audit still exits**1**, now 135diagnostics, stopped at this emitted enumeration.
Every line concerns inherited legacy_metadata or legacy_socket_control. The websocket manifest
mappings show the same undeclared-source/qualified-rename issues and additional public host/helper
symbols without mappings/exemptions. Full failure text is retained, no exemptions or gates were weakened,
and no second-lane implementation/manifest was edited. No post-merge whole-WOO target audit success
is claimed. Original own-slice target audit and six controls remain at24407a9ab0; later affected tests
pass with the own files unchanged. Full127-test suite was not rerun after either integration merge.

## Final delivery refresh

Final code merge `9ec161faaf25e329f9951037e04bfa54657014a1` has actual fetched integration parent
`a1338bc368a19c179baf10f08fab602064136fc9`; fetch and merge exited0. Crate src/tests and own slice
match23966253ed (both scoped diff0). All WOO/journal manifests and crate scripts also match (diff0).
The workspace TOML differs only by unrelated member safe-cex-lighter-sdk, all other values equal;
Cargo.lock differs only by that unrelated package record. Independent WOO-rooted lock traversal
matches every identity/full record/edge, stopped at481package records; all other listed local dependency
inputs are byte-identical. This is a rooted survival proof, not workspace compilation.

Fresh scoped all-target check**0**, extractor**0** (text inspected, zero updates/added/orphans), census**0**
at9ec161faaf: still **32moved,1ported/31divergent; pending183 ->151**. Focused13-test success remains
measured at23966253ed with unchanged source/tests/effective dependency inputs; full127-test success
remains at24407a9ab0. The inherited whole-WOO target audit1 remains explicitly reported at23966253ed;
its crate/manifest/script inputs are unchanged. No full-suite rerun or new control matrix was performed
for this final refresh. The following commit only updates these two evidence files.
