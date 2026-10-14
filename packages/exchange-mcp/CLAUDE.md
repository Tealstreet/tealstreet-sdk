# CLAUDE.md — packages/exchange-mcp

`@tealstreet/exchange-mcp`: a stdio MCP server that trades on one exchange
through safe-cex. Built for the Paradigm team's DIME Terminal, which signs
through a proxy holding the API secret (`EXCHANGE_SIGNER_*`, see `README.md`).
It is in the tealstreet-sdk Copybara slice, so edit it HERE in the monorepo,
never on the mirror.

## Releasing to npm

**Publishing runs on the public tealstreet-sdk mirror.** npm attaches
provenance only to packages published from a public repo, and the
`repository` field in `package.json` must name the repo that publishes:
npm rejected trust for `tealstreet-next` with a bare 400 because this field
names the mirror. The workflow is authored here, at
`mirrors/tealstreet-sdk/.github/workflows/exchange-mcp-release.yml`, and
Copybara carries it across. Never edit it on the mirror.

1. Bump `version` in `package.json` and merge to `master`.
2. Wait for the Copybara export (about 20 minutes), then run
   `yarn release:tag` from this package. It checks that mirror `master`
   carries the new version and tags that commit `exchange-mcp-vX.Y.Z`, which
   fires the workflow there. The workflow typechecks, tests, builds,
   smoke-tests the bundle, checks the pack, and publishes by OIDC with
   provenance. Re-running a published version skips the publish.

npm trusts `Tealstreet/tealstreet-sdk` + `exchange-mcp-release.yml`; set once
by a `@tealstreet` maintainer with
`npm trust github @tealstreet/exchange-mcp --file exchange-mcp-release.yml --repo Tealstreet/tealstreet-sdk`.
0.1.0 was published by hand, because npm cannot trust a package that does not
exist yet.

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
