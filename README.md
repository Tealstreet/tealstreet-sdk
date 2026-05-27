# tealstreet-sdk

A curated workspace containing the Tealstreet exchange SDK (`@tealstreet/safe-cex`),
the Tealstreet CLI, and the shared packages they depend on.

> **This is a mirror.** It is a filtered, read/write view of an upstream
> source-of-truth repository. You contribute here normally (branch, PR, review);
> accepted changes are synced upstream. You do **not** need access to the
> upstream to work here — everything required to build and test lives in this
> repo.

## Top-level consumables

Two things in here are meant to be used directly — start with these:

- **`@tealstreet/safe-cex`** — the exchange integration SDK (market data,
  orders, positions, and WebSocket streams for 28 exchanges). Consumption guide
  - entry points: **[`packages/safe-cex/README.md`](packages/safe-cex/README.md)**.
    Public API is `packages/safe-cex/src/index.ts` (`createExchange`,
    `createPublicExchange`, `getAvailableExchanges`).
- **Tealstreet CLI** — terminal trading client built on the SDK. Dev guide:
  **[`apps/cli/README.md`](apps/cli/README.md)**; user/command docs live at
  <https://docs.tealstreet.io/cli/>.

Everything else (`packages/cli-core`, the other `packages/*`, `thirdparty/*`) is
shared plumbing those two depend on.

## Layout

```text
packages/safe-cex        Exchange integration SDK (the core library)
packages/cli-core        Shared CLI execution + rendering
apps/cli                 Terminal trading client
apps/documentation       Docusaurus docs site (docs.tealstreet.io), incl. CLI reference
packages/*               Shared utilities the above depend on
thirdparty/*             Vendored, name-overridden SDKs (grvt, paradex, …)
```

## Docs

`apps/documentation` is the full Docusaurus site behind `docs.tealstreet.io`.
It builds standalone — no upstream access or workspace packages required:

```bash
cd apps/documentation
yarn install   # (or `yarn install` at the repo root)
yarn start     # local dev server
yarn build     # static build
```

The CLI reference (`docs.tealstreet.io/cli`) lives under `apps/documentation/cli`
and matches the `apps/cli` client in this repo.

## Develop

```bash
yarn install
yarn typecheck
yarn lint
yarn test
```

## Contributing

1. Branch off the default branch (`feature/...`) and open a PR.
2. A maintainer reviews and merges. **Only maintainers can merge** — this is
   intentional.
3. Merged changes are synced upstream and go through a second review before
   landing in the source of truth.

**Releases do not happen from this repo.** There is no publish/release
workflow here and no credentials to perform one; versioning and publishing are
owned entirely by the upstream. Please don't add release automation.

## License

**Proprietary and confidential — NOT open source.** See [`LICENSE`](LICENSE).
The contents of this repository (safe-cex, the CLI, and all packages herein)
are proprietary to Tealstreet. Your access and use are governed solely by the
agreement you signed with Tealstreet (Contributor Agreement / NDA / Commercial
License), which supersedes the `LICENSE` notice in any conflict. Without a
signed agreement you have no rights to the code.
