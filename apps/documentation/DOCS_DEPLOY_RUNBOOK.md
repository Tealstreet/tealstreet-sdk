# Docs Deploy Runbook

## Purpose

This runbook defines the production-safe deployment flow for `apps/documentation` in the monorepo.

Goals:

- Auto-build Preview deploys only for user-facing Docusaurus changes under `apps/documentation`
- Avoid Vercel builds for unrelated monorepo changes, including internal design/agent docs under root `docs`
- Avoid automatic Production deploys
- Promote to Production manually after validation

## Current Deployment Model

- Vercel project root directory: `apps/documentation`
- Skip deploys when no root changes: enabled
- Include files outside root: disabled (not needed — the CLI section lives
  inside `apps/documentation/cli/` as its own Docusaurus content-docs instance,
  no sibling-path access required)
- Ignored Build Step command: `bash ./scripts/vercel-ignore.sh`
- Production branch tracking: `docs-prod` (or another intentionally unused branch)
- Preview branch tracking: all unassigned branches

Keep this project rooted at `apps/documentation`. The root-level `docs/**`
tree is internal-by-default: it contains design docs, implementation notes,
agent guidance, and planning material that should not be treated as
Docusaurus input unless it is deliberately moved into
`apps/documentation/docs/**`.

Do not add package lifecycle hooks (`prebuild`, `prestart`, etc.) that copy
or generate Docusaurus pages from paths outside `apps/documentation`. Those
hooks defeat Vercel's root-scoped deploy filtering and can trigger expensive
Preview builds for unrelated repository changes.

## Function Runtime Settings (Dashboard)

These settings are configured in Vercel project settings, not in `vercel.json`.

- Fluid compute: disabled
- Function CPU/memory: Basic (`1 GB / 0.6 vCPU`)

Important: Vercel does not apply memory/CPU sizing from `vercel.json`; set this in **Project Settings -> Functions** and redeploy.

## `vercel-ignore.sh` Behavior

Location: `apps/documentation/scripts/vercel-ignore.sh`

- Allows `preview` and `development` builds
- For `preview`, builds **only** when changed files include `apps/documentation/**`
- Handles Vercel shallow clones by fetching the previous deployment SHA before diffing
- If changed files cannot be determined at all, defaults to **skip** (avoids wasted builds)
- Blocks `production` builds by default

CLI docs are committed directly under `apps/documentation/cli/`, served by
a dedicated `@docusaurus/plugin-content-docs` instance at
[`/cli`](https://docs.tealstreet.io/cli) (separate from the main `/docs/*`
sidebar and search index). Files named `CLAUDE.md` and `TODO.md` co-exist
with their public counterparts but are excluded from the published site
via the plugin's `exclude` config (`docusaurus.config.js`).

Root-level `docs/**` changes are intentionally ignored by the Docusaurus
Vercel project. If a document should become user-facing help content,
author it under `apps/documentation/docs/**` so the audience boundary and
the deployment boundary stay aligned.

- Optional overrides:
  - `VERCEL_DOCS_FORCE_BUILD=1` to force a build
  - `VERCEL_DOCS_ALLOW_PROD=1` to allow production build
  - `VERCEL_DOCS_FREEZE=1` to skip all builds

## Standard Release Flow

1. Merge docs changes into `master`.
2. Wait for Preview deployment to complete.
3. Validate Preview:
   - homepage
   - representative V3 page
   - representative V2 page
   - one known redirect path
4. In Vercel, promote the validated Preview deployment to Production.
5. Confirm `docs.tealstreet.io` serves the promoted deployment.

Important: when promoting, leave `Use project's Ignore Build Step` unchecked.

## Rollback

If production is unhealthy:

1. Open Vercel Deployments for docs project.
2. Select last known-good deployment.
3. Promote or rollback to that deployment.
4. Re-run smoke checks on `docs.tealstreet.io`.

## Troubleshooting

- No preview deployment created:
  - confirm changed files are under `apps/documentation`
  - confirm Preview branch tracking is enabled
  - confirm ignored build command path is exactly `bash ./scripts/vercel-ignore.sh`
- Deploy unexpectedly skipped:
  - check build logs for `VERCEL_ENV` and ignore script output
  - check `VERCEL_DOCS_FREEZE` is not set
- Unexpected production auto-deploy:
  - confirm production branch is not `master`
  - confirm ignore script still blocks production by default
