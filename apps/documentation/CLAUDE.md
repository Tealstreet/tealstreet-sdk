# CLAUDE.md — apps/documentation

Docusaurus site that ships to **`docs.tealstreet.io`**. Two content roots:

- `docs/**` → main docs at `/docs/*` (V3 + V2 docs, one plugin instance)
- `cli/**` → CLI docs at `/cli/*` (separate `@docusaurus/plugin-content-docs` instance, sidebar in `sidebarsCli.js`)

`**/CLAUDE.md`, `**/TODO.md`, `**/*.private.md` are excluded from the built site via `docusaurus.config.js` `docs.exclude`. Author convention notes inline without leaking.

## RULE — landing a doc on master is NOT publishing it

Merging to `master` only builds a **Preview** deployment on Vercel. Production is a separate, manual step.

### Vercel project

- Project: **`tealstreet-docs-monorepo`** (under team `tealstreet-b565b19f`)
- Root directory: `apps/documentation`
- Production branch: **`prod-docs`** — an intentionally parked branch. It does not move; the Vercel project tracks it so master pushes can't auto-deploy prod.
- Ignored Build Step: `bash ./scripts/vercel-ignore.sh` (gates preview builds to only fire when `apps/documentation/**` changed; blocks production by default).

The ignore step fetches only a missing previous deployment snapshot with
`--depth=1 --filter=blob:none`, then compares the two trees with `--no-renames`.
It does not deepen history or require ancestry. An unavailable endpoint or
failed comparison builds conservatively; branch and override gates run first.

### Release flow (the only correct one)

1. Merge docs changes into `master`.
2. Wait for the Vercel Preview build to complete (it appears in the GitHub PR / commit status).
3. Validate the preview URL (smoke-test the homepage + the page you changed).
4. Promote the preview to production. Two equivalent paths:

   **CLI** (one command):

   ```bash
   /opt/homebrew/bin/vercel promote <preview-url> --scope tealstreet-b565b19f
   ```

   **The CLI path needs the override env.** `vercel promote` REBUILDS the source
   deployment as a production deployment, so the Ignored Build Step runs with
   `VERCEL_ENV=production` and cancels it unless `VERCEL_DOCS_ALLOW_PROD=1` is
   set on the project's Production environment
   (`vercel env add VERCEL_DOCS_ALLOW_PROD production --project tealstreet-docs-monorepo --scope tealstreet-b565b19f`,
   value `1`). Remove it again after the promote so the guard is back. Until
   2026-10-11 the script skipped an ALLOWED production build too (it fell
   through to the catch-all), so three promotes in a row were "Canceled" with
   the env set; that is fixed in `scripts/vercel-ignore.sh`. The promote also
   clones the SOURCE deployment's commit, so a script fix only takes effect on
   a preview built after it landed.

   **Dashboard**: Vercel → `tealstreet-docs-monorepo` → Deployments → pick the validated Preview → "Promote to Production". Leave "Use project's Ignore Build Step" **unchecked** when promoting.

5. Confirm `docs.tealstreet.io` serves the new content (hard-refresh; CDN cache flips fast).

### Why this design (don't "fix" it)

- **`prod-docs` is intentionally far behind master** (currently thousands of commits). Promotions in Vercel pin a specific Preview deployment ID to production — they don't move the `prod-docs` ref. Do not fast-forward `prod-docs` to "catch it up"; that's not how the project promotes.
- The ignore script blocks production builds by default so a forced `vercel --prod` deploy can't accidentally publish without a validated Preview. Override env vars exist (`VERCEL_DOCS_FORCE_BUILD=1`, `VERCEL_DOCS_ALLOW_PROD=1`, `VERCEL_DOCS_FREEZE=1`) but should not be needed in normal flow.
- Root-level `docs/**` (NOT `apps/documentation/docs/**`) is internal design/agent material and is intentionally excluded from the Docusaurus build. If a document should become user-facing help, **move it under `apps/documentation/docs/**`in the same commit** so the audience and the build scope stay aligned. Do not add`prebuild`/`prestart`hooks that copy from outside`apps/documentation` — that defeats the root-scoped ignore filter.

### Rollback

Vercel dashboard → Deployments → previous known-good production deployment → "Promote to Production". Same flow, older artifact.

### Function runtime settings (dashboard-only)

Not in `vercel.json`. Set in Vercel **Project Settings → Functions**:

- Fluid compute: disabled
- Function CPU/memory: Basic (`1 GB / 0.6 vCPU`)

### Troubleshooting

- **No Preview build appeared**: changed files must be under `apps/documentation/**` (the ignore script skips otherwise). Check the build log for `VERCEL_ENV` and ignore script output.
- **Preview build skipped unexpectedly**: confirm `VERCEL_DOCS_FREEZE` is not set on the project.
- **A push to master triggered a production deploy**: production branch is wrong. It must be `prod-docs` (parked), not `master`.

## Authoring conventions

- **CLI section ships from `apps/cli/CLAUDE.md`'s doc-sync table.** When CLI behavior changes, the matching `apps/documentation/cli/**` doc updates in the same commit. The table lives in `apps/cli/CLAUDE.md` — that's the source of truth for which doc pairs with which code surface, do not duplicate it here.
- **Don't author from root `docs/**`\*\* — that tree is for internal design/agent notes only.
- **Don't create barrel-style index pages** that aggregate content from siblings via build hooks (see above re: ignore filter).
- Keep MD/MDX edits localized — the lint step does not run prettier on MD/MDX to avoid known formatting churn (per `lint:format` scope in `package.json`).

## Commands

See [`README.mdx`](./README.mdx) — `yarn start`, `yarn build`, `yarn lint`, locale variants, and repo-root shortcuts (`yarn docs:dev` etc.) live there. Don't duplicate them here.
