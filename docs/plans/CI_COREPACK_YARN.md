# Corepack Yarn preparation in CI

Registry HTTP 503 responses while fetching the pinned Yarn executable currently
fail jobs before dependency installation. `setup-node`'s `cache: yarn` option
also invokes Yarn during Node setup, before a later retry step could run.

Use one shared local action after Node setup in every Yarn job (normal CI,
Rust families/replay/browser, and the existing MCP release job). Remove the
early `setup-node` Yarn probe. The action reads the exact Yarn version from the
root `packageManager`, enables the Corepack shim and prepares the version plus
reviewed SHA-512 digest through Corepack: at most five attempts, each bounded to 60 seconds plus a 5-second forced-kill grace,
with 2/4/8/16-second backoff. Supplying the release digest bypasses the persistently
failing version-metadata endpoint while preserving tarball integrity verification.
The digest is verified against the reachable whole-package metadata and both npm
and Yarn registry tarballs. Version/cache probes disable Corepack networking after
preparation. A wrong version or exhausted retries fails closed; upgrading Yarn
requires updating the reviewed digest.
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

Fresh branch is based on actual #1711 `bd1a69072c7`. Sam has raised this infra
fix above #1709/#1712 because their CI depends on it. Share the frozen reviewed head with
the clock lane for parallel CI; they integrate the actual infra merge before
landing their source changes. Use a fresh separate revert guard and a
separate exact-head merge; report the actual workflow pin to all affected lanes.

Verification: 30 bootstrap/secret-scan tests pass, including retry/timeout
exhaustion and cache ordering; root typecheck44/lint28/tests27 pass from Turbo
cache. Independent correctness and adversarial reviews are clean. Cold empty
Corepack caches on Node22/Corepack0.34.6 and Node24/Corepack0.35.0 prepared the
pinned release with a fetch guard rejecting every non-tarball URL; each made
only one Yarn tarball request, then version/cache probes succeeded offline.
Both registry tarballs are byte-identical and match the published SHA-512.
Full CI is required before merge.
