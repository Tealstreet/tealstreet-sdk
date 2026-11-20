# SDK PR checks against NX callers

AGT-32 adds an ephemeral consumer check before an SDK import. It does not
perform a Copybara import or change release certification.

Every SDK pull request starts one `ubuntu-latest` job, cancelled by a newer
push to that PR. The job resolves NX master once to a full SHA, then checks
out that SHA and the exact SDK PR head with credentials persistence disabled.
A contents-read-only credential scoped to NX is required as
`NX_INTEGRATION_READ_TOKEN`; the SDK's automatic token cannot read private NX.
Fork PRs and a missing credential refuse explicitly before private checkout.
No production credentials, self-hosted runner, artifact upload or write token
are used. The credential is present only during resolution/checkout.

The SDK merge-base-to-head patch is mapped using pinned NX's literal
`TEALSTREET_SDK_FILES`, `TEALSTREET_SDK_MIRROR_FILES` and `ROOT_SCAFFOLDING`
definitions in `mirrors/copy.bara.sky`. Three-way application preserves newer
unrelated NX changes and handles deletions/modes. Conflicts, unsupported path
syntax, uncovered paths and symlinks refuse. This is a disposable local tree;
it is never pushed or labelled as an accepted mirror import.

The job runs the NX unrolled-server reader and Rust release-runbook tests,
then a frozen NX install, UI dependency builds, Next type generation, full
`typecheck:ci`, `lint:ci`, `test:ci` and the web build script. Later gates still
run after earlier gate failures once preparation succeeds; failures stay fatal.
NX root task aliases retain their maintained workspace exclusions.

When candidate WASM inputs match the published pin, the ordinary strict
prebuild validates and installs its assets. Otherwise all beta families are
built from the candidate tree with pinned tools and required optimization.
Only this unpublished integration job runs the exact web `build` script body
after that candidate preparation, without invoking the publication-only
`prebuild` lifecycle. No pin, production guard, package script or release
workflow is changed. No candidate output is uploaded or promoted.

The existing `docker-native-server-pr.yml` already builds both production
Dockerfiles from the SDK root with cache-only output and no push. Its existing
regressions and this gate's tests retain that coverage rather than duplicating
two more image builds.

Standard public-repository hosted minutes are free ([GitHub runner reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)).
The new job has a 120-minute ceiling and executes serially on one runner;
actual duration is unmeasured. A WASM input change can rebuild all beta
families, so this is a real compute cost despite zero billed runner minutes.
The two existing image jobs each retain their 30-minute ceiling. No extra
Vultr runner capacity is consumed.

Offline Git fixtures exercise mapped edits/deletions, preservation of NX-only
changes and conflict refusal. Workflow tests require the reader/runbook and
real task commands, immutable checkout, credential boundaries and cache-only
image builds. Full NX/native builds belong in CI, not on the shared mini.
