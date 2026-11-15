# Agent Guidance

**Tealstreet's own agents:** consult the private maintainer decisions doc before asking about migration, releases, CI or ownership. Follow its recorded decisions and add any new decision there. External contributors can ignore this.

This repository is a Tealstreet mirror. It is a filtered, read/write view of an
upstream monorepo. Contributors and agents work here normally through mirror
branches and pull requests; upstream monorepo access is not required.

This mirror is open-source software under the MIT license; vendored
`thirdparty/*` code keeps the licences listed in `THIRD_PARTY_NOTICES.md`. Do not
add terms that narrow the MIT grant, and never commit credentials: CI scans every
new commit.

## Before Editing

- Start from the latest mirror `master`.
- Use a feature branch for every change.
- Read the nearest relevant `CLAUDE.md` before changing package or app code.
- Keep edits scoped to the requested package, app, or workflow.
- Do not commit directly to `master`.
- Do not add release or publish automation.

## Workflow

Prefer the repository `justfile`:

```bash
just start feat/my-change
just check
just pr
just done feat/my-change
```

Plain Git equivalent:

```bash
git switch master
git pull --ff-only origin master
git switch -c feat/my-change

yarn typecheck
yarn lint
yarn test

git push -u origin HEAD
gh pr create --fill

git switch master
git pull --ff-only origin master
git branch -d feat/my-change
```

If Git reports a conflict, stop and ask in the PR. Do not guess through merge
conflicts unless a maintainer asks you to resolve them.

## Quality Bar

- Run `just check` or the equivalent `yarn typecheck`, `yarn lint`, and
  `yarn test` before marking work ready.
- Let CI and maintainer review be the source of truth.
- Keep commits focused and easy to review.
- Update nearby docs when changing non-obvious behavior.

Merged mirror PRs sync upstream automatically. Do not attempt to access or
modify the upstream monorepo from this mirror.

## Carrying CLEARs Across a Plain Master Merge

A clean master merge can carry existing eligible independent CLEARs at its exact
first-parent SHA to the new head. It never creates a CLEAR. Check the existing
verdicts, reviewer eligibility, any prior carry chain and outstanding FINDINGS;
the proof does not inspect or grant approvals and does not override merge authority.

```bash
node scripts/prove-clean-master-merge.mjs <PR-number> <CLEARed-full-SHA>
# Optionally post the pinned, idempotent proof comment:
node scripts/prove-clean-master-merge.mjs <PR-number> <CLEARed-full-SHA> --post
node --test scripts/prove-clean-master-merge.test.mjs
```

The CLI defaults to `Tealstreet/tealstreet-sdk`. For another GitHub repository,
pass `--repo OWNER/REPO`; the origin must match that repository before any GitHub
request. It fetches master and PR objects without checkout or dependency installation.
It requires exactly two parents (the CLEARed head first, a fetched-master ancestor
second), a clean exact-parent `git merge-tree --write-tree` equal to the head tree,
and byte-identical canonical PR-only merge-base patches. Only text blob IDs and
hunk start offsets are normalized; paths, modes, context, changed bytes, hunk counts
and binary patches remain exact. Conflicts, manual merge edits, additional commits,
changed patches, shallow/ambiguous history, unavailable evidence and a moving PR
head fail closed. A failed proof requires normal independent review at the new head.

A proof PASS does not waive new-head required CI checks. For write paths,
`Rust / combined consumers` must actually RUN and conclude SUCCESS at the new
head; SKIPPED/selected-out jobs and a green aggregate alone are insufficient.
Maintainers still decide whether all existing reviews and other merge gates pass.
