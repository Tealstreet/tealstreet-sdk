# CLAUDE.md — @tealstreet/script-api-web

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md) §2.3.

## The implementation deliberately lives in `apps/web`, not here

This package exports **contracts only** — `WebApi` (script-facing) and
`WebAdapter` (host-implemented). The adapter implementation stays in
`apps/web/src/components/customScripts/scriptAdapter.ts` because it needs the
jotai store, `exchangeAccountController`, and app atoms; importing those here
would invert the dependency direction.

This is not a stylistic preference — it is the B3 finding in
[`docs/custom-scripts/CONVERGENCE-AUDIT-v1.md`](../../docs/custom-scripts/CONVERGENCE-AUDIT-v1.md).
`apps/cli` also depends on this package and must not acquire a React or
`apps/web` runtime dependency through it.

## `react` and `components` are empty and carry NO autocomplete

Do not "fix" them by typing them concretely. Two independent reasons:

1. Typing `react: typeof import('react')` pulls `@types/react` into
   `scripts/generate-monaco-types.ts`'s dts-bundle-generator output, which
   emits a triple-slash reference directive. The bundler wraps that in
   `declare module '@tealstreet/script-api-web' { … }`, where such a directive
   is invalid — corrupting the seeded `~/.tealstreet/scripts/types.d.ts` for
   every user.
2. A host-side `declare module` augmentation does **not** rescue this.
   `WEB_TYPES_DTS` is generated from this package's `src/index.ts` alone, so
   an augmentation in `apps/web` never reaches the bundle. Making these fields
   real requires teaching the generator to concatenate a host-supplied
   augmentation slice **and** targeting the declaration rather than
   `index.ts`'s re-exported alias (merging into an alias is unsupported, and
   there is no precedent in this repo).

Until both land, they exist for shape fidelity with SPEC §2.3. Do not
document them to users as working.

## `ui.*` is sync and returns USD notional

SPEC §2.3 sketches `Promise<number | undefined>`. The real resolvers
(`UITokenResolvers` in `@tealstreet/cli-core`, implemented at
`apps/web/src/components/cli/uiTokenResolvers.ts`) are **synchronous** and
return **token strings** like `"$100"` — the CLI's substitute layer requires
complete tokens, not bare numbers. `WebAdapter` is the string → number seam;
`parseUIToken` does the conversion.

`getOrderSize` / `getQuickSize` are **USD notional, not contracts**. Easy to
get wrong and expensive when wrong.

`parseUIToken` returns `undefined` for percent tokens: a percent-sized quick
button has no notional meaning without a position. It also guards
`Number('') === 0`, so a bare `"$"` reads as unavailable rather than as a
zero-size order.

## Version bumps are CI-enforced

Changing `src/api/WebApi.types.ts` without bumping `WEB_API_VERSION` fails
`yarn check:api-versions` (`scripts/check-api-version-bump.mjs`), wired as its
own step in `ci.yaml`. Raising the version is strictly permissive —
`targetCheck` only rejects when the runtime provides _less_ than a script
requires, and an omitted `webApiVersion` is treated as `0.0.0`.

After changing the surface, run `yarn generate:types` and commit the
regenerated `packages/script-authoring/src/script-types-bundle.ts`; a drift test asserts byte
equality and will fail otherwise.
