# Docs Migration Hardening Report

## Completed Migration Work

- Imported `tealstreet-docs/documentation` into `apps/documentation` in the monorepo.
- Removed generated/editor artifacts from the imported app.
- Added monorepo scripts in root `package.json`:
  - `docs:dev`
  - `docs:build`
  - `docs:serve`
- Added docs task entries to `turbo.json`:
  - `documentation#build`
  - generic `start` and `serve` tasks.

## Build Verification

- Root install completed from `tealstreet-next`.
- Root docs build command succeeds:
  - `yarn docs:build`
- Docusaurus still reports pre-existing broken links/anchors in legacy docs content, but build output is generated successfully.

## Prune Audit Results

- Candidate filter used:
  - last meaningful commit before `2023-01-01`
  - no commits by user identities (`daxx`, `sean`, `daxxtriaz`, `122421568`)
- Reference checks run against non-static documentation sources before deletion.
- Results:
  - Auto-pruned (safe unreferenced): **39 files**
  - Deferred (still referenced or needs manual review): **131 files**
- Full list is recorded in `apps/documentation/prune-audit.json`.

## Vercel Status

- Kept fallback project intact:
  - `tealstreet-docs-sib3` root directory restored to `documentation`.
- Reused existing team project for monorepo docs (no new project creation):
  - `tealstreet-docs-monorepo`
- Set monorepo docs project settings:
  - `framework: docusaurus-2`
  - `rootDirectory: apps/documentation`
  - `nodeVersion: 22.x`
- Deployment created successfully:
  - `https://tealstreet-docs-monorepo-olqdbdcg9-tealstreet-b565b19f.vercel.app`
  - alias: `https://tealstreet-docs-monorepo.vercel.app`

## Remaining Cutover Steps

1. Connect `tealstreet-docs-monorepo` to `tealstreet-next` Git repo in Vercel UI (CLI git connect failed with repository access error).
2. Validate V3/V2 routes in browser (CLI smoke checks return 401 due deployment protection on non-custom domains).
3. After parity confirmation, move `docs.tealstreet.io` domain to `tealstreet-docs-monorepo`.
4. Keep `tealstreet-docs-sib3` unchanged as rollback target during the stabilization window.
