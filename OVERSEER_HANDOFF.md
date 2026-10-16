# Rust port — handoff to the tealstreet-sdk overseer

Written 2026-10-16, when `sdk-rs` landed on `tealstreet-next` master. This file
is monorepo-authored scaffolding, so it arrives at the mirror's repository root.
It carries what is not derivable from the code: the order of operations, the
gates that are red and by how much, and the traps that have already cost days.

The behavioural contract is unchanged and lives where it always did:
`sdk-rs/parity/MANIFEST-SPEC.md` §§14-19. Cite those by their `<a id>` anchor;
never restate them.

## 1. State, with the commits

| Thing | Where |
| --- | --- |
| The port | `tealstreet-next` master, `f769aa8323` |
| Integration branch | `feat/safe-cex-rust`, fast-forwarded to the same commit — no divergence left |
| This mirror's `sdk-rs` | **absent** |
| Mirror visibility | PUBLIC, so its Actions minutes are free and its logs are world-readable forever |

`mirrors/copy.bara.sky` pins `ref = "master"`, and `sdk-rs/**` has been in
`TEALSTREET_SDK_FILES` since 2026-10-14, so the export direction is now
permitted to carry it. **It will not arrive on its own** — see §2.

## 2. The seed, which is the first thing to do

`merge_import` three-ways the whole slice, and a file absent on the mirror reads
as a deletion there, so an existing directory never arrives through a normal
sync. `packages/oems-core` sat in this slice for a month proving exactly that.

The fix is a one-time seed: a PR on this mirror that copies `sdk-rs/` byte for
byte from monorepo master, admin-merged. Not Copybara, not a hand-fed baseline,
not `--last-rev`. `mirrors/README.md` → "Adding a Mirror" has the wording.

Until that lands, a Rust job here compiles nothing, and a green badge on it
means nothing — which is worse than no job, so do not add one first.

**The seed would have failed this repo's own `secret-scan` job before
2026-10-16.** This repo had no `.gitleaks.toml` at all and the slice did not
carry one, so that job fell back to gitleaks' DEFAULT rules — and `sdk-rs`
carries 332 synthetic findings outside `target/` (Hardhat dev keys, exchange-doc
example keys, generated signing fixtures). A seed commit adds all of them at
once, on a PUBLIC repo. `.gitleaks.toml` is now in the slice, in all FIVE places
the path set is encoded (`TEALSTREET_SDK_FILES`, the two `paths:` blocks in
`sdk-secret-scan.yml`, its `SDK_PATHS`, and `slice_mono_paths` in
`copybara-mirror-sync.sh` — `yarn check:mirror-paths` fails until they agree).
Confirm the file is present at this repo's root before pushing a seed branch.

**The second thing the seed hits is this repo's `.gitignore`.** It carries
`*.log`, and the port tracks **2,963 `.log` files, 24.2 MB**, under `sdk-rs/` —
gate evidence the parity audits cite by path. With `*.log` in force they are
unstageable here while the monorepo tracks them, so the two sides' `sdk-rs`
trees could never be equal, `health` would report `in_sync=no` forever and every
export would retry the same files. The scaffold now negates them
(`!sdk-rs/**/*.log`). Check `git check-ignore -v` on one of those paths before
seeding; a seed that silently drops 2,963 files is the wedge, not the fix.

Recorded as debt rather than fixed here: root `CLAUDE.md` says never commit
generated artifacts, and 24.2 MB of gate logs is exactly that — the 712 MB
`packages/tealscript/reports/` purge of 2026-09-12 is the precedent. They are
evidence the audits cite, so deleting them is a decision for whoever owns the
parity record, not a cleanup to do in passing.

## 3. One authoring home, and it is now this repo

Authoring the same change in both repos makes both sides edit the same hunks
under different SHAs. The export then hits an intermediate monorepo state of
files this side already has in final form, DIFF3 writes conflict markers, and
`verify_match` kills the run — every 20 minutes, for as long as it takes someone
to notice. That wedged tealchart for four hours on 2026-09-04.

So after the seed: **`sdk-rs` is authored here and Copybara imports it to the
monorepo (`mirror-to-mono`).** Nothing may author `sdk-rs` in `tealstreet-next`
again, including a well-meaning one-line clippy fix.

The monorepo still owns, and must keep owning:

- `packages/safe-cex` and the rest of the TypeScript — this slice carries it
  read-only in practice; it is the parity reference, and changing it here means
  changing the thing the oracles are measured against.
- `mirrors/**`, including this file and this repo's `.github/workflows/ci.yml`.
  Contributors cannot edit that workflow here (CODEOWNERS + branch protection)
  and should not try; edit `mirrors/tealstreet-sdk/.github/workflows/ci.yml`
  in the monorepo and let the export carry it.

## 3a. This repo's Actions policy allows GitHub-owned actions ONLY

Measured 2026-10-16: `allowed_actions: selected`, `github_owned_allowed: true`,
`verified_allowed: false`, `patterns_allowed: []`. So a step naming anything
outside `actions/*` makes the whole **run** fail at startup — and a startup
failure is per run, not per job, so it takes `ci` and `secret-scan` down with
the offending job.

That happened the moment the Rust job arrived: `dtolnay/rust-toolchain@stable`
turned three consecutive exports into `startup_failure` and **this mirror ran
nothing for about seven hours**, including the nine package suites the monorepo
had already stopped running on the strength of this repo running them. Nothing
alerted — which is §7's missing failure routing, demonstrated rather than
predicted.

The fix keeps the policy as it is: `ubuntu-latest` ships rustup, and `rustup
show` in a directory carrying `rust-toolchain.toml` installs the pinned
toolchain and components, so the action was never doing anything the image
cannot. `mirrors/scripts/mirror-workflow-actions.test.mjs` now fails
`yarn test:mirror-guards` on any non-`actions/*` step in any mirror workflow.
If a third-party action is genuinely needed, the repo setting has to change
deliberately and the pattern must be recorded in that guard.

## 3c. The import direction re-rooted the whole port, and that is now fixed

The first mirror-side commit — the seed itself — came back into the monorepo as
**11,280 files at `mirrors/tealstreet-sdk/sdk-rs/`**: a second copy of the entire
port inside the scaffolding directory, commit `2902d7fdf2`, carrying a normal
`SdkMirror-RevId` trailer and deleting nothing, so no guard fired.

Cause: `ROOT_SCAFFOLDING` in `mirrors/copy.bara.sky` is the import move's
`paths=` filter, and its exclude list named only `apps`, `packages` and
`thirdparty`. Anything else at the mirror's root is treated as scaffolding and
re-rooted under `mirrors/<slice>/`. `sdk-rs` joined the slice on 2026-10-14 and
that list was never updated, so the export worked (no `paths=` on that side) and
only the import was wrong. Fixed by adding `sdk-rs/**` and `infra/**` there and
in `scaffolding_excluded_roots`; `yarn check:mirror-paths` pins the two
encodings against each other and now reports 5 roots.

**`.gitleaks.toml` was the same mistake in miniature, made by me.** I added the
monorepo's root allowlist to the slice so this repo's scan would stop falling
back to gitleaks' defaults; the import re-rooted it to
`mirrors/tealstreet-sdk/.gitleaks.toml`, which is exactly where every other
mirror-root config file already lives (`.gitignore`, `package.json`,
`turbo.json`). It is now scaffolding, like them, and the slice entry is gone.
**Config files that belong at this repo's root are authored under
`mirrors/tealstreet-sdk/` in the monorepo, never added to the slice as root
paths.**

The lesson for anything added to the slice later: a slice path needs an entry in
`ROOT_SCAFFOLDING`'s exclude list in the SAME change, or the first import after
it duplicates the tree.

## 3b. The first real run found a genuine hole: the SDK reached outside the slice

`sdk-rs/safe-cex-transport/src/lib.rs` declares `mod frames;` with a path
attribute pointing at `infra/tradetunnel/src/frames.rs`, and `how.rs` asserts
the workspace `serde_json` pin is byte-identical to that crate's manifest.
Neither file was in the slice, so the first real Rust run here failed at
`cargo fmt --all --check` — rustfmt resolves modules before formatting, so it
never reached clippy or build.

That is worth stating as more than a CI fix: **a published SDK that compiles
only inside the monorepo is not publishable**, and no monorepo gate could see
it, because there the file is simply present. Only running in this repo found
it. Both files are now in the slice, and only those two: the broker-id
enforcement in `relay.rs` and `http_request.rs` stays private. `frames.rs` is
63 lines of wire shapes this SDK already speaks.

The manifest is there purely so one test can compare pins. If that reads wrong
later, the alternative is to move that assertion into the monorepo's
`scripts/rust-ci-wiring.test.mjs`, which is never published, and drop the
manifest from the slice. Either is defensible; what is not is skipping the
assertion when the file is absent.

**Measurement caveat, so the next person does not repeat it:** the 33-of-36
figure in §4 counted the GATE SCRIPTS' own inputs. It did not count what the
crates compile, and this is what that gap looked like. Before claiming another
gate can move, check the crate sources for path attributes and `include_str!`
reaching above `sdk-rs/`.

## 4. What can run here today

Measured on 2026-10-16 against `TEALSTREET_SDK_FILES`: **33 of the 36 Rust gate
scripts have every input inside this slice.** The three that did not were moved
inside it rather than widening the path set — widening it would have bypassed
`sdk-secret-scan.yml`, which only scans NEW commits:

- `sdk-rs/scripts/rust-wasm.mjs`, `rust-wasm.test.mjs`, `sdk-rs/rust-wasm.json`
  (were under `.github/`) — `wasm:lint`, `wasm:build`, `check:rust-wasm-policy`.
- `sdk-rs/parity/extract-ts/scripts/check-dependency.mjs` (was
  `scripts/check-parity-extractor-dependency.mjs`) — `parity:extract`.

The wasm policy audit no longer names one workflow: it reads every candidate
that exists — the monorepo's `rust.yml`, the monorepo-authored mirror `ci.yml`,
and that same file as seen from inside this repo — and holds each one that
declares a `wasm:` job to the same three mandatory steps. It fails closed if NO
candidate declares the job, so the tier cannot be deleted from both.

`rust-ci-wiring:check` stays monorepo-only by nature: it asserts things about
`rust.yml` itself.

**The gate scripts are now declared here** — all 35 of them (`parity:*`,
`registry:check`, `crypto:*`, `layers:check`, `routing:check`, `wasm:*`,
`venue-oracle:*`, `worker-oracle`, `ffi:*`, `check:rust-wasm-policy`), with
`sdk-rs/parity/extract-ts` as a workspace and `tsx` as a devDependency. The
lockfile was regenerated the documented way, by installing over the EXISTING
mirror lock so shared ranges resolve identically rather than re-floating: the
diff is 15 added lines, `tsx`, `smol-toml` and an `esbuild` range merge, and no
resolution was removed. Yarn 1's `--frozen-lockfile` does not error on an absent
entry — it silently resolves fresh — which is why that matters.

Proved in a real clone of this repo: `yarn layers:check` exits 0.

**`parity:extract --check` CANNOT pass here, and it is not a wiring problem.**
Three of the extractor's nine source roots are outside the slice and absent from
this repo — `packages/safe-cex-web`, `packages/safe-cex-server` and
`packages/safe-cex-react-native`, the P3b candleCache roots. Measured in that
clone: `Would update 35 manifests; 112 added, 20 orphaned symbols`, which is the
extractor correctly reporting that the TypeScript those rows map to is missing,
not drift. Two ways out, and it is a publication decision rather than a fix:
add those three packages to the slice (five path-set encodings, `gitleaks dir`
over each, and a seed per package since they already exist), or leave the
coverage gate monorepo-only and accept that it runs after the import. Everything
else — build, clippy, fmt, layers, routing, registry, the venue oracles, the
crypto and worker oracles — has all its inputs here.

## 5. The three red gates, with the measurement

Do not take these as "nearly green". They are the reason `rust.yml` is red on
monorepo master right now.

- **`parity:check`** — runs to completion (the `thread_local` abort is cleared)
  and emits **320 row-level findings, all one class**: a rename note must name
  its exact Rust target. Concentrated in **lighter 150** and **bybit 122** (85%
  of the total), then woo 4, hyperliquid 4, base 3, okx 2, coinbase 2, weex 1.
  Bulk-fixable per venue.
- **`parity:accepted`** — the `LoaderFuture` abort is cleared (PR 7996). It now
  fails on `safe_cex_exchange::exchange::SharedExchangeOptions`
  private-construction Clone/Copy.
- **`parity:accepted:controls`** — the accepted-set mutation-control runner.
  **Until it passes, a green `parity:accepted` is not trustworthy**, because the
  assertions are not proven to discriminate.

**`crypto:oracles:check` cannot be judged locally at all.** It needs
CI-provisioned `SAFE_CEX_TYPESCRIPT_PATH`, `SAFE_CEX_CRYPTO_ORACLE_DEPS` and
`SAFE_CEX_ZKLINK_NATIVE_ORACLE`; the generators for all three are in the slice
(`sdk-rs/safe-cex-types/ci-deps.cjs`,
`sdk-rs/safe-cex-crypto/scripts/ci-deps.cjs`, `.../zklink-native.cjs --prepare`),
so a job here can provision them exactly as monorepo `rust.yml` does. Only CI
settles it. Its last genuine defect was real: the type-only `Market.isPerpetual`
addition invalidated oracle source-hash provenance.

Green on this tree, for reference: `cargo fmt --check`, `cargo build --workspace
--locked`, host clippy, wasm clippy, `parity:extract --check`, `registry:check`,
`routing:check`, `layers:check`, `check:rust-wasm-policy` (17/17),
`crypto:lighter:check`, and all four mirror slice-safety gates.

## 6. Work that is committed and NOT merged

Three branches in `tealstreet-next`. Each is a real deliverable with a named
reason for being out; none is lost.

| Branch | Why it is out |
| --- | --- |
| `fix/rust-prototype-realm-parity-index` | Conflicts on `sdk-rs/parity/exempt.toml` (union-resolvable) and `sdk-rs/parity/manifest/exchanges/bybit/bybitspot.exchange.ts.toml` (row-level annotations — needs the lane that wrote them, never an automatic union) |
| `fix/mexc-divergent-closure` | Conflicts on `mexc.exchange.ts.toml` plus real source divergence in `safe-cex-venue-mexc/src/exchange.rs` and `exchange_tests.rs` |
| `feat/rust-bybit-startup` (`0ad92af38b`) | Merges clean, held out on purpose: its own lane labelled it an unfinished checkpoint, not behavioural acceptance |

`exempt.toml` and `**/GATE_EVIDENCE.md` are the ONLY files safe to union-resolve
(append-only entries; re-parse the TOML afterwards and refuse on a duplicate
`rs` key). `MANIFEST-SPEC.md`, any `CLAUDE.md` and the per-row manifest TOMLs are
living specs — sending those back to their lane has twice found a real bug,
including eight duplicate hyperliquid `exempt` rows with divergent reason text.

## 7. The two steps still open in the monorepo

From `docs/plans/PUBLIC_MIRROR_CI_v1.md`, whose §6 ordering is now partly done
(step 1, landing on master, is complete):

1. **Delete the five self-hosted Rust jobs from `tealstreet-next`'s
   `.github/workflows/rust.yml`** once this mirror runs them green. Delete, not
   relocate — two copies of a matrix drift. The hidden cost is
   `scripts/rust-ci-wiring.test.mjs`: 909 lines pinning that workflow step by
   step, which has to be retargeted at the mirror's workflow in the same change.
   Master has already executed this exact shape for the tealchart slice
   (`0db7397883`): `paths-ignore` the packages, delete the steps, let the public
   mirror run them. It is proven, not proposed.
2. **Failure routing, which does not exist yet.** Nobody watches this repo's
   Actions tab. A red run here must open an issue in `tealstreet-next` — the
   `scheduled-tests.yaml` pattern, `TEST_ISSUE_TOKEN` plus an `@claude` mention.
   That needs a token provisioned on a PUBLIC repo, so it is Sam's to create.
   **Until it exists, moving a job here is equivalent to deleting it.**

## 8. Traps

- **Never `--baseline-for-merge-import`, never `COPYBARA_DISABLE_BASELINE_CHECK=1`.**
  It asserts the destination already holds everything up to a revision; when
  that is false, `merge_import` reads the far side's newer content as deletions
  and rolls it back after the push, where Copybara cannot recover it.
- **Never enable Dependabot here.** Both sides owning one `yarn.lock` resolved a
  postcss bump two ways on 2026-08-16 and wedged the tealchart mirror until it
  was fixed by hand. Bumps belong in the monorepo and flow outward.
- **Git history is not ground truth when auditing a lost import.** Clobbered
  commits stay ancestors of master, so `git log` reports work as present while
  the code is gone from the tree. Compare three blobs per file: at the commit,
  at its parent, and in the working tree.
- **This repo's history is public and was never scanned.** The secret scan is
  new-commits-only by design, because the tree's Hardhat dev keys and
  exchange-docs example keys would fail every run. `gitleaks dir sdk-rs` cleared
  the port for publication (366 findings, all synthetic).
- **A green monorepo PR no longer says a mirrored slice is good**, because those
  suites gate after the merge. Check this repo's run before deploying.
- **Measuring clippy on a workspace that does not compile reports nothing.**
  `--keep-going` skips crates that fail to build, so the site count CLIMBS as
  crates start compiling. During the 2026-10-14 cleanup the figure went
  76 → 71 → 8 → 539 → 55 → 78 → 36 → 0. Quote a count only from a run over a
  workspace `cargo check --workspace --all-targets` already accepts.
- **On a laptop, the Volta yarn shim swallows child exit codes and returns 126**,
  so a real failure reads as "command not executable". Run
  `node <volta>/yarn/1.22.21/bin/yarn.js <script>`. CI is unaffected.
