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

## Immediate dependency-download follow-up

#1713 landed as `04bec2ac52f`; its frozen bootstrap stays unchanged. Actual clock
CI then failed fetching cosmiconfig8.3.6 with a tarball HTTP503 after preparation
succeeded. Fresh `fix/ci-yarn-network-retry` starts from that actual infra merge.

Keep the existing exact Yarn-version/OS/architecture/lockfile-keyed cache restore
before installation; switch to restore-only and expose its key/path/hit to one
shared install action. Save a missing cache immediately after a successful root
install so later unrelated replay failures do not discard the warmed cache.
Replace all five install steps with that action. Use Corepack Yarn with the
existing frozen lockfile and network timeout, plus prefer-offline and JSON output.

Use at most three attempts with 5/10-second backoff. A shared runner merges
stdout/stderr into one ordered stream, preserves live logs and original failure
exit, and requires positive structured network-error evidence while resolving or
fetching packages. Retry only transient HTTP408/429/5xx or recognized transport
errors, and only when every fatal error qualifies. No retry after linking, build
or lifecycle activity, on root preinstall hooks, unknown/unstructured fatal data,
nonstandard exits/signals, integrity, lockfile, engine or script failures.
Disable retry for nonstandard lockfile source types: Git prepare scripts may run
during fetching, outside the ordinary build phase. The current 2,330 resolved entries
are ordinary HTTP(S) tarballs. Never
delete node_modules/cache or weaken checksum/frozen-lockfile checks on retry.

Review the plan and implementation independently, including architecture gate:
one shared CI install/cache choke point, no venue-specific policy. Fault-inject
transient and permanent network errors, recovered and exhausted attempts, fatal
mixtures, lifecycle/root-preinstall guards and cache ordering/save conditions.
Prove old single-shot install red and retry path green on the same injected503.
Run bootstrap/secret regressions, root checks, shell/YAML/whitespace checks and
full CI. Freeze/share head promptly, then fresh separate guard and separate merge;
report actual workflow pin before dependent lanes claim fresh CI.

Follow-up verification: 39 bootstrap/install/secret-scan checks pass, including
the actual validation→resolve→fetch prefix, unknown/prototype phase labels,
nonstandard source kinds and original-single-shot RED/retry GREEN. Real isolated
Yarn 1.22.21 against a local failing/recovering tarball server also proves original
HTTP 503 rejection and shared-runner recovery on the second install. Root typecheck (44), lint (28) and test (27)
checks pass from Turbo cache; YAML/whitespace checks pass. Full CI remains required.
