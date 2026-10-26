# CLAUDE.md — packages/exchange-mcp

`@tealstreet/exchange-mcp`: a stdio MCP server that trades on one exchange
through safe-cex. Built for the Paradigm team's DIME Terminal, which signs
through a proxy holding the API secret (`EXCHANGE_SIGNER_*`, see `README.md`).
It is in the tealstreet-sdk Copybara slice, so edit it HERE in the monorepo,
never on the mirror.

## Releasing to npm

**Publishing runs on the public tealstreet-sdk mirror.** npm attaches
provenance only to packages published from a public repo, and the
`repository` field in `package.json` names that repo, which the release job
checks. The workflow is authored here, at
`mirrors/tealstreet-sdk/.github/workflows/exchange-mcp-release.yml`, and
Copybara carries it across. Never edit it on the mirror.

**To release: tealstreet-sdk → Actions → "exchange-mcp release" → Run
workflow → patch, minor or major.** The job takes npm's `latest`, bumps it,
stamps the result into `package.json` for that build only, typechecks, tests,
builds, smoke-tests, checks the pack, publishes by OIDC with provenance, and
only then pushes the `exchange-mcp-vX.Y.Z` tag on the commit it built.

**The version lives on npm and in tags, never in a commit.** `package.json`
holds `0.0.0-development` on purpose: mirror `master` is protected
(`mirror-maintainers` only, reviewed PR plus green `ci`), so the workflow's
token cannot commit a bump, and a committed bump would also have to
round-trip through Copybara. Do not "fix" the placeholder. Pushing an
`exchange-mcp-vX.Y.Z` tag by hand also releases exactly that version; a
dispatched run refuses a version that already has a tag or is already on npm.

npm trusts `Tealstreet/tealstreet-sdk` + `exchange-mcp-release.yml`, set on
the package's npm **Settings** page under Trusted Publisher with **Allow npm
publish** checked. Without that box a trusted publisher may only stage a
release. **Do not use `npm trust github`:** npm 11.13 sends no allowed actions
and the registry answers with a bare 400. 0.1.0 was published by hand, because
npm cannot trust a package that does not exist yet; 0.1.1 was the first
release from the workflow, with SLSA provenance.

## The bundle

Rust accounts load `dist/wasm/rust/safe_cex_wasm.cjs` and its sibling WASM.
Build the pinned Rust engine from the repository root with `corepack yarn -s
rust:build-wasm` before the package build; the existing release job does this.
The `rust-smoke.mjs` probe copies release-owned assets outside the checkout and
starts a real native Coinbase catalog, so workspace resolution cannot mask a
missing artifact. `EXCHANGE_ENGINE=rust` remains a compatible input; the beta registry always
selects Rust and rejects OUT names with typed Unsupported/NotSent before I/O. Tests selected by `SAFE_CEX_WASM_TESTS=1`
exercise real protocol tool calls and account state, using documented inputs.
Tools are chosen from `exchange.loadCapabilities()`, which creates the native
engine without starting it; a failed load lists tools from base answers and
surfaces on the first call. Failed native startup is retried with a fresh account on the next tool call;
concurrent calls share the replacement startup. Keep shutdown reading
`instance.exchange` at call time, since destructuring would retain the failed
account. Rust accounts admit `EXCHANGE_SIGNER_HEADER` only for aliases whose
shared construction policy enables PayloadSigner. Their trusted setup installs
the existing JS callback as the injected provider and retains REST-only private
feeds; generic options contain neither callback nor raw credentials. Other Rust
families still reject it before issuing requests. Provider callbacks remain
local to this process; no serialized callback or local-secret fallback is added.
Rust accounts keep their public feeds active because native ticker tools read
the current engine store. Do not apply the TS-only public-socket suppression to
Rust; Binance's REST seed alone would freeze its quote tools. The protocol test
replays actual recorded Binance socket traffic and requires changed tool quotes.
Historical MCP WASM fixtures explicitly mock beta admission to retain their
protocol assertions for OUT reference venues; production admits no TS adapter.
Keep native REST timers active as well as sockets:
BreakoutProp has no stream and its tools depend on periodic reconciliation.
A real native protocol regression requires changed quotes after its REST poll.
History UTC mapping preserves the SDK's non-enumerable incompleteness marker.
For known partial history, tool text JSON is `{ requestId, rows, completeness:
{ incomplete: true } }`, using the owning JSON-RPC request ID. Unflagged
responses retain the existing array shape; absence does not prove exhaustive
history. Empty partial results still carry metadata, and failed reads remain
errors. Consumers must handle the partial envelope before calculating totals
or recording coverage.

The license generator includes the pinned engine's non-dev Cargo dependency
closure and the existing adapted zkLink notice. Run it after building WASM.
Its `cargo metadata --locked --filter-platform wasm32-unknown-unknown` command
may fetch missing locked crate sources: workspace/dev manifests can be needed
for metadata even when the WASM build never downloaded them. A fresh cache
therefore needs Cargo registry access. Notice selection still excludes dev
edges and uses the same license allowlist and pinned lockfile. These notices
retain existing workspace dependencies; changing Rust dependency licensing
still needs the repository's usual review.

- **Bundled JS and owned WASM, no runtime dependencies.** `tsup` inlines everything, including
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
