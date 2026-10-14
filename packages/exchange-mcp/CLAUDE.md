# CLAUDE.md — packages/exchange-mcp

`@tealstreet/exchange-mcp`: a stdio MCP server that trades on one exchange
through safe-cex. Built for the Paradigm team's DIME Terminal, which signs
through a proxy holding the API secret (`EXCHANGE_SIGNER_*`, see `README.md`).
It is in the tealstreet-sdk Copybara slice, so edit it HERE in the monorepo,
never on the mirror.

## Releasing to npm

**Publishing runs from the monorepo, not the mirror.** Tags do not cross
Copybara, a release commit authored on the mirror would collide with the next
export, and `@tealstreet/cli` already publishes from here by trusted publishing.
The cost: npm generates provenance only from a public repo, so packages
published from this private one carry none.

1. Bump `version` in `package.json` and merge to `master`.
2. Tag that commit `exchange-mcp-vX.Y.Z` and push the tag.
   `.github/workflows/exchange-mcp-release.yml` typechecks, tests, builds,
   smoke-tests the bundle, checks the pack, and publishes by OIDC. Re-running a
   published version skips the publish.

**First-time setup, once per package:** npm must trust the workflow. A
maintainer of the `@tealstreet` scope runs
`npm trust github @tealstreet/exchange-mcp --file exchange-mcp-release.yml --repo Tealstreet/tealstreet-next`.
If npm refuses because the package does not exist yet, they publish the first
version by hand (`yarn build && npm publish --access public` from this
directory), then run the trust command.

## The bundle

- **One file, no runtime dependencies.** `tsup` inlines everything, including
  every `@tealstreet/*` workspace. Those are not on npm, and a runtime
  dependency on one is what made `npm install -g @tealstreet/cli` fail with
  E404. The release workflow refuses any `dependencies` field.
- **Licences ship beside it.** `scripts/third-party-licenses.mjs` reads the
  esbuild metafile and writes `dist/THIRD_PARTY_LICENSES.md`. It fails the build
  on any licence outside its allowlist, evaluating SPDX `AND`/`OR`/`WITH`
  properly. A package that ships no licence file gets the canonical text from
  `scripts/licenses/`. MPL-2.0 is allowed because the files ship
  unmodified and the output names their source. `@tealstreet/grvt-client` is
  UNLICENSED and allowed by Sam's 2026-10-08 decision, the same one recorded in
  the mirror's `scripts/check-licenses.mjs`. Add to the allowlist only with a
  decision behind it.
- **gitleaks flags the bundle; that is expected.** The hits are third-party
  default public keys inside bundled dependencies (ethers' provider defaults),
  Polymarket contract addresses and a PEM-parsing regex. The source tree scans
  clean, and the sdk-secret-scan workflow covers it.
- **`yarn smoke` boots `dist/index.js`** with `EXCHANGE=bybit` and no
  credentials, and requires `initialize` and `tools/list` to answer. It also
  fails on any non-protocol line on stdout, which would corrupt the MCP stream.
- **Lighter needs `dist/wasm/poseidon_crypto.wasm`.** poseidon-crypto's node
  loader reads it from `__dirname`, which ESM lacks, so the tsup banner defines
  `__dirname` and `onSuccess` copies the file beside the bundle. Without both,
  Lighter fails with "Failed to initialize WASM" while every other venue works.
  `scripts/poseidon-probe.ts` is bundled into `dist/` the same way, and
  `yarn smoke` runs it, because the stdio check never touches signing. The Go
  runtime prints through `console.log`, which `src/stdio.ts` sends to stderr.
- **Third-party code inside our own workspaces** (CCXT in `ccxt-compat`, the
  RingBuffer in `data-structures`, Go's `wasm_exec.js` in `poseidon-crypto`) is
  listed from its `THIRD_PARTY_NOTICES.md` section. A workspace that gains
  vendored code needs a section there, or the licence file omits it.
