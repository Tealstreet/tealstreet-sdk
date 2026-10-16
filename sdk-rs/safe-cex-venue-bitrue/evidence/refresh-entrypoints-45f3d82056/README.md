# Superseded first implementation measurements

These raw gates were measured at `45f3d82056db724775dead3b9acb37ad2809d39e`,
plus the individually documented mutation patches where indicated. They are
preserved as historical evidence, not final proof for the revised margin row.
The 26-case native witness passed, three wrong-behavior controls each produced
a named runtime assertion failure (101) then passed after restoration (0), and
scoped check, host clippy, wasm clippy, formatting and extractor all exited 0.
Exact commands and exits are in `gates.json`; the case floor ended at margin
case3 `emit-throw`.

Sub-agent review found gaps the original witness did not assert: `async fn`
delayed the margin emission until polling, its shared Error representation
omitted JavaScript stack/prototype details, and its public helper error enum
needed an explicit mapping. Canonical index validation was not run here; the
mapping concern was a static review finding, not a measured validator exit.

Commit `ecce21189f234cd2d6727e7b11c0f855b8ecf331` corrects these findings:
eager invocation returning a ready future, generic retention of the source
error-constructor's complete returned/thrown value, removal of the extra enum,
and native pre-poll/error-identity assertions. The revised 27-case witness and
its fresh controls/gates are recorded in
[the corrected evidence directory](../refresh-entrypoints-ecce21189f/README.md).
