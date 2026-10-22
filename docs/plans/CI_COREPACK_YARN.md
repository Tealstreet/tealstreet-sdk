# Corepack Yarn preparation in CI

Registry HTTP 503 responses while fetching the pinned Yarn executable currently
fail jobs before dependency installation. `setup-node`'s `cache: yarn` option
also invokes Yarn during Node setup, before a later retry step could run.

Use one shared local action after Node setup in every Yarn job (normal CI,
Rust families/replay/browser, and the existing MCP release job). Remove the
early `setup-node` Yarn probe. The action reads the exact Yarn version from the
root `packageManager`, enables the Corepack shim and prepares it through
`corepack yarn --version`: at most five attempts, each bounded to 60 seconds,
with 2/4/8/16-second backoff. A wrong version or exhausted retries fails closed.
Restore the existing kind of dependency cache only after preparation succeeds;
key it by OS, architecture, Yarn version and lockfile. All Yarn commands remain
explicit `corepack yarn`. No dependency-install retry, version change, release
trigger, permission or application behavior change is included.

Plan first, then implementation and fault-injection regressions, independent
correctness/adversarial review and full CI. Tests use mocked Corepack, timeout
and sleep executables: immediate success, transient 503 recovery, bounded
exhaustion, per-attempt timeout, wrong-version rejection and downstream cache
and workflow ordering. Run existing secret-scan regressions and root checks;
do not run local WASM builds or load gates. Architecture gate: shared CI
bootstrap choke point, no venue-specific cross-cutting implementation.

Fresh branch is based on actual #1711 `bd1a69072c7`. Current overseer order is
#1709 clock first, then this infra PR real-integrates its actual merge and
passes fresh CI, then #1712 history. Use a fresh separate revert guard and a
separate exact-head merge; report the actual workflow pin to all affected lanes.
