# SDK PR checks against NX callers

AGT-32 adds an ephemeral consumer check before an SDK import. It does not
perform a Copybara import or change release certification.

Every SDK pull request starts one `ubuntu-latest` job, cancelled by a newer
push to that PR. A single NX checkout resolves master, and its full SHA is
recorded; the exact SDK PR head is checked out too, both with credentials
persistence disabled. The SDK's automatic token cannot read private NX, so a
read-only NX deploy key (`sdk-pr-nx-integration` on tealstreet-next) is
required as `NX_INTEGRATION_DEPLOY_KEY`: it reads that one repository and
nothing else, and every account that can push an SDK branch already reads NX.
Fork PRs and a missing key refuse explicitly before private checkout.
No production credentials, self-hosted runner, artifact upload or write
credential are used. The key is present only during admission and checkout.
Private NX command output stays in mode-600 runner-temporary logs and is never
uploaded; public logs report the gate and its result. Failures retain nonzero
status. Reproducing the pinned command in an authorized NX checkout provides
the detailed private output.
The required context remains named `ci`: a cheap hosted aggregate requires
the unchanged SDK validation steps (`SDK checks`) and this reusable job.
Failed, cancelled or skipped PR gates refuse. Master pushes and manual mirror
CI keep their existing checks without running the PR-only consumer job.

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
The aggregate adds one job with a five-minute ceiling and no checkout/install.
The two existing image jobs each retain their 30-minute ceiling. No extra
Vultr runner capacity is consumed.

Offline Git fixtures exercise mapped edits/deletions, preservation of NX-only
changes and conflict refusal. Workflow tests require the reader/runbook and
real task commands, immutable checkout, credential boundaries and cache-only
image builds. Full NX/native builds belong in CI, not on the shared mini.
