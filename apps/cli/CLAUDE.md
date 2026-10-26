# CLAUDE.md — @tealstreet/cli

Terminal-based crypto trading interface built on Bun. Interactive REPL for direct exchange interaction.

**Remote signing lives in `services/exchange-server/`.** The legacy in-CLI `serve` mode + `serve-config.ts` (instance keypair, paired sessions, relay config) was deleted in the listener rebuild — only `src/tasks-store.ts` survives from that subtree for `~/.tealstreet/tasks.json` persistence. **The local loopback listener (`src/listener/`) is the WS+HTTP surface the browser talks to** on `127.0.0.1:53219`. Bound when the REPL launches, closed on REPL exit. See [`operations/listener.md`](../documentation/cli/operations/listener.md) for the end-to-end spec — wire shapes, auth, kill switches, ops.

## ⚠️ RULE — keep the user-facing docs in sync

**User-facing CLI docs live in [`apps/documentation/cli/`](../documentation/cli/)** as a dedicated Docusaurus content-docs instance and ship to `docs.tealstreet.io/cli` on the next build. They are committed directly — no sync script, no mirror.

**When you change anything covered by a public doc, update the matching doc in the same commit.** This is not optional and it is easy to forget — a feature can be fully shipped (merged, released) yet invisible to users because the docs never moved. If you touch behaviour in the left column, the right-column file changes in the **same commit/PR**, full stop.

> **Before you commit any `apps/cli/src/**`behaviour change, ask: "which row of this table did I just touch?"** If the answer is any row, that doc file must be in your staged diff. Also update the matching part of`README.md` (contributor view) and the [Logging](#logging) section of this file when they overlap. Three homes, one source of truth — they drift the moment one is skipped.

Concrete pairings:

| You changed                                                                                                                                  | Update                                                                                                                                                                                                                                         |
| -------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `cmd:exec` / `cmd:output` / `cmd:done` wire frames, `hello`/`welcome`, close codes, listener auth, `POST /hook/<id>` shape, kill-switch keys | [`cli/operations/listener.md`](../documentation/cli/operations/listener.md)                                                                                                                                                                    |
| New / renamed / removed REPL command, new flag, new alias                                                                                    | [`cli/reference/commands.md`](../documentation/cli/reference/commands.md)                                                                                                                                                                      |
| Logging behaviour — `console.*` routing, operational-noise suppression, severity taxonomy (`log-classify.ts`), `print`-above-prompt          | [`cli/reference/output.md`](../documentation/cli/reference/output.md) + the [Logging](#logging) section below                                                                                                                                  |
| Output surfaces — top alert bar (`status-line.ts`), bottom footer (`footer.ts` / `footer-segments.ts`), scroll region (`scroll-region.ts`)   | [`cli/reference/output.md`](../documentation/cli/reference/output.md), [`commands.md#footer`](../documentation/cli/reference/commands.md#footer), [`config-files.md`](../documentation/cli/operations/config-files.md)                         |
| Write-op audit log / command tracing — `audit` command, `runId` grouping, `[fill]` attribution, rotation                                     | [`commands.md#audit`](../documentation/cli/reference/commands.md#audit) + [`config-files.md`](../documentation/cli/operations/config-files.md)                                                                                                 |
| Sizing / price grammar changes                                                                                                               | [`cli/reference/sizing-and-prices.md`](../documentation/cli/reference/sizing-and-prices.md)                                                                                                                                                    |
| Chain operators / loops / repeat / if / wait / retry behavior                                                                                | [`cli/reference/workflow.md`](../documentation/cli/reference/workflow.md)                                                                                                                                                                      |
| New / removed `$constant` or math grammar                                                                                                    | [`cli/reference/variables.md`](../documentation/cli/reference/variables.md)                                                                                                                                                                    |
| `set @x` / `sets @x` / `track max/min @x` / alias / capture syntax                                                                           | [`cli/reference/aliases-and-vars.md`](../documentation/cli/reference/aliases-and-vars.md)                                                                                                                                                      |
| Recipe-shaped patterns (trailing stop, bracket, fan-out, TWAP, listener-from-web, webhook-from-TradingView)                                  | [`cli/quickstart.md`](../documentation/cli/quickstart.md) and the matching `cli/recipes/*.md` deep-dive                                                                                                                                        |
| Hook / webhook subsystem                                                                                                                     | [`cli/operations/hooks-and-webhooks.md`](../documentation/cli/operations/hooks-and-webhooks.md)                                                                                                                                                |
| Trusted-clients allowlist + pairing UX                                                                                                       | [`cli/operations/trusted-clients.md`](../documentation/cli/operations/trusted-clients.md) and `operations/pairing.md`                                                                                                                          |
| Recording / SQLite schema changes                                                                                                            | [`cli/operations/recording.md`](../documentation/cli/operations/recording.md)                                                                                                                                                                  |
| Release pipeline / homebrew / binaries                                                                                                       | [`cli/operations/releases.md`](../documentation/cli/operations/releases.md)                                                                                                                                                                    |
| Config files under `~/.tealstreet/`                                                                                                          | [`cli/operations/config-files.md`](../documentation/cli/operations/config-files.md)                                                                                                                                                            |
| Cross-trade engine — daemon, store, listener broadcast, web-CLI safety warning                                                               | [`cli/operations/cross-trade.md`](../documentation/cli/operations/cross-trade.md) + [`commands.md#cross-trade`](../documentation/cli/reference/commands.md#cross-trade) + [`config-files.md`](../documentation/cli/operations/config-files.md) |
| Safety guards (simulation / fatfinger / whitelist)                                                                                           | [`cli/reference/safety.md`](../documentation/cli/reference/safety.md)                                                                                                                                                                          |

Files excluded from the published site (via `docs.exclude` in `apps/documentation/docusaurus.config.js`): `**/CLAUDE.md`, `**/TODO.md`, `**/*.private.md`. Convention notes can stay co-located without leaking.

See [`apps/documentation/cli/CLAUDE.md`](../documentation/cli/CLAUDE.md) if one exists for the authoring convention; the dedicated `/cli/` plugin instance and its sidebar live in `apps/documentation/sidebarsCli.js`.

## Brands — one source, two binaries

`src/brand.ts` is the only place a product name, executable name, config
directory, web-app URL, docs URL or release repo is spelled. `scripts/build.sh
<brand>` inlines `TEALSTREET_BRAND` with `--define`; dev runs and tests carry no
define and resolve to `tealstreet`, so every pinned `~/.tealstreet` and
`tealstreet-*` expectation in the suite stays valid. The `disrupt` brand logs in
through `https://app.disrupt.trade` (the apex is a landing page and 404s on `/api/cli/*`), stores under `~/.disrupt/`, and self-updates
from `DisruptTrade/cli`.

**That URL is the entire auth change.** The web app picks the Auth0 tenant from
the request host (`tenantFromHost` in `apps/web/src/utils/auth/auth0.ts`), so
`/api/cli/login` on `disrupt.trade` returns a proxy token whose `userId` is
already `disrupt:<sub>`. The relay, tradetunnel and `/api/cli/*` routes are
shared and verify the shared `privateProxyKey`; do not add a brand switch to
any of them. **Infrastructure names are NOT branded** — `TEALSTREET_*` env vars,
`--tealstreet-proxy`/`--tealstreet-tunnel` flags, the relay URLs and the HoW
`Origin: https://preview.tealstreet.io` header (a WAF requirement) stay the
same on every brand. Renaming them would break docs and scripts for nothing.

Adding a config-dir or display-name literal anywhere under `src/` is a brand
leak: route it through `BRAND`. One known limit, accepted for v1: both brands
bind the listener on `127.0.0.1:53219`, so they cannot run at the same time.
**The disrupt build must never print a tealstreet.io URL** — its `docsUrl` is
the `DisruptTrade/cli` README, and `src/brand.test.ts` greps the disrupt brand
for `tealstreet.io`. The internal HoW `Origin` header and the listener's origin
allowlist are not user-visible and are exempt.

**Docs for a whitelabel are GENERATED, never hand-maintained.**
`scripts/brand-docs.mjs <brand> <out>` rebrands `apps/documentation/cli` (drops
front matter and `:::` admonitions so GitHub renders it, rewrites binary name,
config dir, env prefix, release repo, hosts and the routing flags, writes a
`README.md` index in sidebar order) and REFUSES if any page still names the
source brand. Pages whose content genuinely differs — install channels — are
whole-file overrides in `docs-brands/<brand>/`; keep them in step with the
source page when it changes. The release workflow pushes the result to the
brand's release repo under `docs/` on every `<brand>-cli-v*` tag, authored as
the token's owner, and `BRAND.docsUrl` points there. This is also why the REPL
and `account add` help on a whitelabel show `--proxy` / `--tunnel` /
`--ws-route`: both spellings are accepted on every brand (`cli-core`'s
`getCliBrand().flags` picks which one help shows), so the docs never have to
print a `--tealstreet-*` flag.

**Release:** `BRAND=disrupt yarn release:patch` pushes `disrupt-cli-vX.Y.Z`.
`cli-release.yml` derives brand, binary name and repo from the tag prefix and
skips the npm and Homebrew steps for anything but tealstreet; the Windows smoke
job skips disrupt tags because its install path is npm. Both brands share
`package.json`'s version. The release step publishes with
`DISRUPT_CLI_RELEASE_TOKEN` for disrupt and `TEALSTREET_CLI_RELEASE_TOKEN`
for tealstreet: the latter is scoped to the Tealstreet org and 403s on
`DisruptTrade/cli`, which is how v0.10.17 ended up published by hand from a
local `build.sh disrupt` of the tagged commit.

## Runtime

`src/rust-engine.ts` initializes the Rust browser artifact from a Bun embedded
asset; standalone binaries need no workspace or extracted WASM file. Build
`corepack yarn -s rust:build-wasm` from the repository root before source runs
or CLI compilation. The `*.wasm.test.ts` consumer tests (selected with
`SAFE_CEX_WASM_TESTS=1`) prove saved-account native state and an isolated
compiled binary. They do not prove live routing or authenticated connectivity.

**Bun** (not Node.js). Compiled to standalone binaries via `bun build --compile`. Cross-platform: darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64.

**Windows shipped 2026-09-27 and is deliberately partial.** `bun build --compile --target=bun-windows-x64` works off an unmodified tree, so the build was never the obstacle — the POSIX assumptions around it were. Two of them survive:

- **`bun build --compile` appends `.exe` itself** on a Windows target. The asset name, the `SHA256SUMS` line and the cached filename must all carry it or the npm launcher `spawnSync`s a path Windows will not execute. `assetName()` in `scripts/install-binary.cjs` is the single derivation; `src/install-binary.test.ts` pins it. Don't re-derive the suffix from `process.platform` at a second call site.
- **`tealstreet update` replaces the Windows binary by RENAME, in `src/windows-update.ts`.** Windows locks a running `.exe` — it cannot be overwritten or deleted, but it can be renamed and the open handle follows. Hence stage beside, rename current to `.old`, rename the download into place, delete `.old` on the next launch (`cleanupStaleUpdate()`, called from `index.ts` right after `interceptConsole()`). **The rollback in `swapInPlace` is the load-bearing part**: if the second rename fails and the first is not undone, the user is left with no `tealstreet.exe` at all. `windows-update.test.ts` pins that, and it fails without the rollback. A stale `.old` is also cleared before the swap, because Windows `rename` does not overwrite and one leftover would otherwise break every future update.
- **`detectInstallMethod()`'s probes must not use shell-specific syntax.** They carried `2>/dev/null`, a POSIX redirect that `cmd.exe` reads as a path under a non-existent `/dev`, so both probes threw on Windows and detection fell through to `binary`. Use `stdio: 'pipe'`.

**Windows install paths are smoke-tested; the REPL is not.** `.github/workflows/cli-windows-smoke.yml` runs on a GitHub-hosted `windows-latest` runner after each release — npm install, `--version`, `--help`, `account list`, plus a direct `.exe` download — so the install path is no longer unverified. It is a separate workflow from `cli-release.yml` on purpose: every other job here is `[self-hosted, tealstreet]`, and a hosted-runner outage must not fail a release that already published.

**The REPL's rendering is still unverified.** It drives DECSTBM scroll regions, a top alert bar and a bottom footer over Node `readline`. That should be fine under Windows Terminal / PowerShell 7 (ConPTY) and is expected to render badly in legacy `cmd.exe`. Nobody has run it — there is no Windows machine on the team and CI has no Windows job, so a `--version` smoke test is the most that can be automated. Treat a Windows REPL bug report as plausible rather than surprising.

### The npm package has NO `dependencies`, and that is deliberate

`files` ships three things: `bin/tealstreet.cjs`, `scripts/install-binary.cjs` and the README. The launcher downloads the platform binary and `spawnSync`s it, and between them those two files require **node builtins only**. Nothing in the tarball imports a package.

Everything the CLI actually uses is compiled INTO the binary by `bun build --compile`, so it is build-time, and it lives in `devDependencies`. **Do not move it back.** Ten of those entries are `@tealstreet/*` workspace packages at `"*"` which are not published to npm, so listing them as runtime deps made `npm install -g @tealstreet/cli` fail with `E404 @tealstreet/cli-core` for every user — resolution dies before `postinstall` ever runs, so the `--optional` flag on it saves nothing. Reported from Discord on 2026-09-24, broken since the package was first published.

## Tradetunnel / HoW WebSocket Runtime

CLI accounts that normalize to HoW/tradetunnel use the shared
`@tealstreet/http-over-websocket` package through safe-cex. That path is
load-bearing: in Node/Bun it uses a header-capable WebSocket constructor,
attaches `Origin: https://preview.tealstreet.io` plus a browser-like
`User-Agent`, and forces HTTP/1.1 via vendored `ReconnectingWebSocket` ALPN
options. Direct safe-cex exchange WebSockets to `*.tealstreet.io` use the same
header/ALPN branch in `packages/safe-cex/src/exchanges/base.ws.ts`. Do not add a
CLI-local tradetunnel socket or route tradetunnel through bare Bun native
`WebSocket`.

`src/ws-shim.ts` is separate: it protects other `*.tealstreet.io` WebSocket
clients from Bun's HTTP/2 upgrade behavior by using `tcp-websocket`. It is not
the HoW/tradetunnel auth path and should not replace `HOWWebSocket`.

## Architecture

```
src/
├── index.ts                 # Entry point (Commander.js setup)
├── repl.ts                  # Interactive REPL (readline-based)
├── config.ts                # Config: ~/.tealstreet/config.json
├── exchange-factory.ts      # Exchange instantiation from config
├── exchange-manager.ts      # Multi-exchange lifecycle
├── auth.ts                  # OAuth2 login flow (port 9876 callback)
├── updater.ts               # Version checking + auto-update
├── logger.ts                # Rate-limited, deduplicated logging
├── scroll-region.ts         # Single owner of the terminal DECSTBM scroll region (top bar + footer reserve through it)
├── status-line.ts           # Top alert bar (transient warn/error/critical, auto-dims) — reserves the TOP rows
├── footer.ts                # Bottom status footer surface (always-on) — reserves 2 BOTTOM rows (divider + content); flicker-free deduped redraw; setFooterRenderer injects content
├── footer-segments.ts       # Pure built-in footer segment renderers (connection/account/balance/pnl/…) + DEFAULT_SEGMENTS
├── platform-adapter.ts      # Standalone platform adapter (non-web)
├── commands/                # CLI commands
├── listener/                # Loopback :53219 WS + HTTP (see apps/documentation/cli/operations/listener.md)
│   ├── index.ts             # startListener / stopListener
│   ├── http.ts              # /health + POST /hook/:id + WS upgrade
│   ├── ws.ts                # WS handshake + post-hello dispatch
│   ├── auth.ts              # Origin allowlist + JWT email decode
│   ├── state.ts             # listener.json + trusted-clients.json
│   ├── prompt.ts            # REPL approval prompt via adapter
│   ├── clients.ts           # REPL `clients list` / `revoke`
│   ├── account-broadcast.ts # state:accounts + fs.watch + dedup
│   ├── tasks-broadcast.ts   # state:tasks + fs.watch
│   ├── hooks-store.ts       # hooks.json + secret/token plumbing
│   ├── hook-repl.ts         # REPL `hook list/create/revoke/rotate/show`
│   ├── webhook-relay.ts     # Hosted webhooks: outbound WS to news-gateway, ack-gated delivery runner
│   ├── webhook-repl.ts      # REPL `webhook [status]/claim/release`
│   ├── adapter-context.ts   # AsyncLocalStorage adapter override
│   ├── config.ts            # 3 kill-switch readers
│   └── handlers/
│       ├── cmd.ts           # cmd:exec → cmd:output → cmd:done
│       ├── account.ts       # mutate:account upsert/delete (LWW)
│       └── hook.ts          # handleHookFire (POST /hook/<id>); hooks are managed from the REPL only
├── record/                  # Market data recording (SQLite)
│   ├── database.ts          # SQLite DB (~/.tealstreet/recording.db)
│   └── recorders/           # Recorder implementations
├── tasks-store.ts           # ~/.tealstreet/tasks.json persistence
├── cross-trade-store.ts     # ~/.tealstreet/cross-trade.json persistence (StoredCrossTradeConfig = safe-cex CrossTradeConfig + id/updatedAt/deletedAt; LWW + tombstones, mirrors config.ts:accounts)
├── commands/cross-trade.ts  # `tealstreet cross-trade list|add|enable|disable|remove|run` Commander surface
├── audit-log.ts             # ~/.tealstreet/audit.log append-only write-op trail (TSBUG-107; off by default, `audit` cmd)
├── scripts-store.ts         # ~/.tealstreet/scripts/*.ts file-based loader (SPEC §5.1) + initial-load promise for the dispatcher race-fix
├── script-dispatcher.ts     # parseScriptInvocation — pure helper for runStep's `script:<name>` arm (SPEC §6.1)
├── bun-transpile.ts         # Bun.Transpiler primitive for `runScriptFromHost` (fn-ref skips it; here for forward-compat)
└── utils/
```

## Operating Modes

### One-shot commands must tear down explicitly

`runSingleCommand` ends at `teardownOneShot`, and that function calls
`process.exit`. **This is load-bearing, not belt-and-braces.** `stopExchange`
alone leaves the process alive: `virtualClock` re-arms a 5-minute sync timer
that nothing else clears, and the HoW tunnel sockets hold a ping timer —
`disposeHOWWebSockets` exists and had zero callers. The REPL never showed this
because every REPL exit path already calls `process.exit`. Shipped 0.10.5 hung
on every `tealstreet <command>` that touched an exchange; a user had to Ctrl-C
out of `balance`. `src/one-shot-teardown.test.ts` pins the wiring.

A rejected API key is TERMINAL on this path (exit 1) for any command whose
`requiresConnection` is not `false` — `start()` has no auth gate, so a venue
that rejected the key still "starts", and running the command anyway is what
printed a zeroed balance table that read as an empty account. Registration is
lazy, so the gate calls `ensureCommandsRegistered()` before asking the registry.

`yarn workspace @tealstreet/cli torture` exercises the whole one-shot surface
against real accounts from the keys sheet, in an isolated `$HOME`, and fails on
anything that hangs or leaks a stack trace. Nobody had tested non-REPL mode
before it.

**REPL mode** (default): Interactive terminal with readline. Watch modes (`wa`/`wp`/`wo`/`wm`) use alt screen + raw mode.

**Listener mode** (passive, always-on by default): The loopback `Bun.serve` on `127.0.0.1:53219` is bound during REPL startup and torn down on REPL exit. Gated by `config.listener.enabled`. See [`operations/listener.md`](../documentation/cli/operations/listener.md).

## Crash reporting (`src/sentry.ts`)

Sentry project `tealstreet-cli`. **Gated on `TEALSTREET_CLI_VERSION` being
defined**, which `scripts/build.sh` inlines via `--define` — so `bun run
src/index.ts` never reports and only released binaries do. `TEALSTREET_TELEMETRY=0`
opts out; `TEALSTREET_FORCE_SENTRY=1` forces it on for local verification.

Load-bearing details:

- **`uncaughtException` must flush before `process.exit`.** The handler in
  `index.ts` awaits `flushCliSentry()` (2 s cap) before exiting — a bare
  `process.exit(1)` drops the crash we most want to see.
- **This binary runs on user machines against their own accounts**, so the
  shared restrictive Sentry `dataCollection` baseline is used, `server_name` is
  deleted, and the HTTP/fetch integrations are filtered out (`Http` plus
  `NodeFetch` in Sentry v10 / `Fetch` in v11; they would attach exchange request
  URLs). `beforeSend`/`beforeBreadcrumb` redact key-shaped values (32+ chars of
  unbroken base64/hex) and any sensitive-looking key name. Don't relax these to
  "get better context" — the context is exchange API credentials.
- Uses **`@sentry/bun`**, not `@sentry/node`. The runtime is Bun and the output
  is a `bun build --compile` binary; `@sentry/node`'s OpenTelemetry
  auto-instrumentation is not reliable there.
- Same 2-per-5-minutes throttle as the web/mobile clients.

User-facing doc: [`cli/operations/config-files.md`](../documentation/cli/operations/config-files.md) → "Crash reporting".

## Config Files

All in `~/.tealstreet/`:

- `config.json` — Account credentials, default symbol, auth token, the `listener.*` kill-switch block (`enabled` / `accountSync` / `remoteExec`, all default true), and the `footer.*` block (`enabled` on unless explicitly false, `intervalMs`, `segments[]`) for the bottom status footer.
- `tasks.json` — Persisted long-running tasks
- `cross-trade.json` — Cross-trade engine configs (`StoredCrossTradeConfig[]`). Shape extends safe-cex's `CrossTradeConfig` (master/copiers) with `id` + `updatedAt` + `deletedAt` for LWW + tombstones. The CLI daemon (`tealstreet cross-trade run`, wired in B3) and the web side both read this same shape so the listener bridge can sync edits across runtimes. Chase mode is accepted at `add` but skipped at `run` in v1 — Market and LimitSlippage only.
- `vars.json` — User variables (`set @x …`) — `Record<string, string>`
- `aliases.json` — User aliases (`alias <n> "<cmd>"`) — `Record<string, string>`
- `aliases-sync.json` — alias changes queued for the server plus its revision cursor (`src/alias-sync.ts`, shared logic in cli-core `aliasSync`). The SERVER orders changes (`supabase/cli_aliases.sql`); never add device timestamps. Only user changes through `FilePreprocessStore`'s overrides are queued — `applyRemoteAliases` and construction-time loading must not be, or every pull echoes back as a push.
- `cli-export.json` — Default target for the `export` / `import` REPL
  commands (vars + aliases + reserved `binds`).
- `listener.json` — Persistent `listenerId` UUID (web clients pin this on first approval)
- `trusted-clients.json` — Approved web `clientId` → `{label, origin, approvedAt}`
- `hooks.json` — Webhook registry (id + secret + command). **Contains shared secrets.**
- `scripts/` — User-authored custom scripts (one `.ts` file per script) plus three seeded files: `tsconfig.json` (only-if-missing), `types.d.ts` (regenerated on every CLI launch so ambient types stay in sync with the running binary's script-API surface — content comes from `@tealstreet/script-authoring`'s `script-types-bundle.ts`, see Phase 6 gotcha below), `hello.ts` (only-if-missing example). Loaded via dynamic `import()` by `src/scripts-store.ts`; the directory IS the registry (no JSON manifest). Invoked via the `script:<name>` REPL prefix — see SPEC §6.1 + `apps/documentation/cli/reference/commands.md#scripts`. REPL `script reload` re-scans. Phase 5's dispatcher arm in `repl.ts:runStep` awaits `awaitScriptsLoaded()` so a cold-REPL `script:foo` doesn't race the fire-and-forget initial load.
- `cli.log` — Log file (cleared on startup)
- `market-data.db` — SQLite for market data snapshots

### `login` / `logout` / `auth` exist as subcommands AND REPL commands

They were REPL-only (`repl.ts` `processSingleCommand`), so `tealstreet login`
exited with `unknown command 'login'` — while the proxy/tunnel failure path prints
exactly `Tealstreet proxy/tunnel requires authentication. Run "login" first.`
That instruction is unqualified, so a reader naturally tries it in the shell.
Both surfaces now accept the same three names; keep them in sync if either moves.

`auth` (alias `whoami`) is the fast way to distinguish "never logged in" from
"token expired" — the proxy rejects both identically, but the second reports
`Logged in … Token expires in: 0 days`.

### `account add` must use the shared account contract

Manual `account add` and account import surfaces must call
`@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig` before saving.
That safe-cex module owns exchange-specific required fields (for example
Paradex `apiKey` + `paradexAddress` + `subkeyAddress`) and routing
normalization. Do not add CLI-local `SUPPORTED_EXCHANGES` maps or hand-built
credential schemas.

### `account add` must be able to set routing

`platform-adapter.ts` has always honored `tealstreetProxy` / `tealstreetTunnel` /
`proxyRegion` on a saved account (it forwards them through
`normalizeExchangeAccountConfig`), but `account add` had **no flags to set them** —
so every CLI-added account was written with routing off and egressed **direct**.
For an IP-whitelisted key that is an immediate rejection (`OKX 50110 invalid IP`,
`Binance 401 … request ip: …`), and the symptom looks like a bad key rather than a
routing gap. `--tealstreet-proxy`, `--tealstreet-tunnel` and `--proxy-region` close
that. Any future account-creation surface must expose the same fields.

Note the proxy/tunnel path then requires `login` — an account can be added
non-interactively but not _used_ through the proxy until the OAuth flow has run.

## Auth Flow

1. CLI opens browser → `{web}/cli-login?callback=http://localhost:9876/callback`
2. User authenticates via Auth0
3. Browser redirects back with JWT
4. CLI stores token in config

## MCP: there is no CLI server

Tealstreet runs exactly ONE MCP server, the hosted connector at
`apps/web/src/app/api/mcp/` (`https://preview.tealstreet.io/api/mcp`). The
CLI's own stdio MCP command was removed as a direct cutover: two registries
meant two tool vocabularies, two scope models and two renderers of the same
reads, kept in step only by parity tests. Do not add an MCP server back to the
CLI; extend the hosted connector instead.

## `--json` mode: stdout is one document

`--json` anywhere in argv (`jsonMode.ts`) is detected right after
`interceptConsole()`, before the update check. It calls
`reserveStdoutForJson()`, which makes the console interceptor send info output
to **stderr**. Patching `process.stdout.write` does NOT work: Bun's native
console bypasses it, which is why the switch lives in `logger.ts`. Results go out
through `emitJsonAndExit`, never `console.log`, whose dedupe and rate limit would
eat repeated documents. Exiting after the flush is deliberate: `fireCheckin`'s
fetch has no timeout and would otherwise hold an agent's process open.
`account list --json` uses an ALLOWLIST of fields; configs also hold secondary
keys, passphrases, seeds and bot tokens. The REPL half of `tealstreet commands`
comes from `cli-core`'s `buildCommandCatalog`, the same one the hosted MCP serves.

**`exec --json` is `runSingleCommandJson` in `repl.ts`**, a sibling of
`runSingleCommand` that must keep the same steps. Every adapter print is captured
through an ADAPTER CONTEXT `printSink`, and exchange code (which reports through
console, not the adapter) through `setConsoleTap`. That tap is load-bearing:
Bybit answers a rejected key with HTTP 200 and only LOGS it, so without it the
run printed a zeroed balance and reported `OK`. `connectToAccount` reports a
failed connect by printing, never throwing, so its code comes from the captured
line (`connectFailureCode`). Codes live in `oneShotResult.ts`; the docs table in
`reference/commands.md#json` must list every one.

## Startup checkin (web mount gate)

Every CLI process fires a single best-effort `POST /api/cli/checkin`
right after `interceptConsole()` in `src/index.ts`. The web side reads
`settings.cli.checkins[<webPlatform>]` to decide whether to mount its
`CliListenerWorker` — without that gate, every user's web app was
hitting `ws://127.0.0.1:53219/ws` on every page load and logging 3+
connection errors regardless of whether they actually run the CLI.

Contract:

- `src/checkin.ts:fireCheckin()` — module-level latch (at most one
  POST per CLI process), no-op when not logged in, no-op on
  unsupported platforms (BSD etc.), swallows network failures.
- Sends the stored proxy token as `Authorization: Bearer <token>`;
  the web route verifies it with the `privateProxyKey` public
  counterpart via `apps/web/src/app/api/utils/verifyProxyToken.ts`
  (NOT Auth0 `/userinfo` — the CLI's token is RS256-signed by us,
  not an Auth0 access token).
- Backend write is a narrow `fn_upsert_cli_checkin` JSONB merge
  (`supabase/users_app_settings_cli_checkin.sql`) — NOT the
  destructive full-blob upsert that `POST /api/settings` uses, which
  would wipe the rest of the user's settings.

If you rename either side: update both `apps/cli/src/checkin.ts` and
`apps/web/src/app/api/cli/checkin/route.ts` in the same commit and
update the `/api/cli/checkin` reference in this paragraph. The
co-located web doc is `apps/web/src/components/cliListener/CLAUDE.md`
→ "Mount gate — don't remove it".

## Remote Signing

Remote signing lives in `services/exchange-server/`. The CLI no longer
participates in that trust model directly, but the flow is documented here
because adjacent CLI work (auth/login) interacts with the same server:

The client **always** strips API secrets before sending init messages to the
exchange server. The server never holds keys — all request signing is
delegated to the client over WebSocket.

**How it works:**

1. Client strips secret fields from the init payload — only the allowlisted
   public fields (see `INIT_FIELDS` in `ExchangeServerWorker.ts`) are sent.
2. Server detects the missing secret → creates a `WebSocketRemoteSigner` for
   that account.
3. When the exchange adapter needs a signature, it calls `remoteSigner.sign()`
   which sends a `{ t: 'sign-req' }` frame back to the browser over the same
   WebSocket.
4. Browser signs with the real key → returns `{ t: 'sign-res' }`.
5. Server fires the signed request to the exchange.

**Security model:**

- The client decides whether to sign — the server cannot request keys.
- Write operations (place order, cancel, set leverage) are placed directly
  from the client path when using hosted/serve mode; the server only handles
  read-only request signing for state fetches.
- Even a compromised server cannot trick the client into signing write
  operations.

**Key files:**

- `services/exchange-server/src/remote-signer.ts` — `WebSocketRemoteSigner`
  class (sign request/response transport, failure tracking, 5s timeout).
- `packages/safe-cex/src/remoteSigningAdapter.ts` — Axios adapter (server-side)
  that routes through the remote signer.
- `apps/web/src/utils/shim/worker/ExchangeServerWorker.ts` — Browser-side
  WebWorker: owns the single shared WS, multiplexes accounts over
  MessagePort, holds the `INIT_FIELDS` allowlist that gates what is sent to
  the server, and handles incoming `sign-req` by routing to the correct
  account port.
- `apps/web/src/controllers/exchangeController/exchangeAccountController.ts` —
  Account lifecycle logic that decides when the server worker is used.

## Logging

**User-facing mirror: [`cli/reference/output.md`](../documentation/cli/reference/output.md) → "Output kinds".** Changing any row here means updating that doc + `README.md` in the same commit (see the doc-sync table above).

| Method                             | Terminal                                                              | Log file (`~/.tealstreet/cli.log`) |
| ---------------------------------- | --------------------------------------------------------------------- | ---------------------------------- |
| `logger.info/warn/error/debug()`   | No                                                                    | Yes                                |
| `console.log/info()`               | Above prompt, **unless operational** (then file-only)¹ — rate-limited | Yes                                |
| `console.warn/error()`             | Above prompt + mirrored on the top status bar²                        | Yes                                |
| `console.debug()`                  | No                                                                    | Only when `DEBUG=1`/`true`³        |
| `print()` / `originalConsoleLog()` | Yes (no rate limit, no suppression)                                   | No                                 |

¹ Lines matching **operational** patterns (`[trackLastReceived]`, `[BaseReconnectingWebSocket]`, `[HOW] Latency`, ping/keepalive noise, balance dumps) are suppressed from the terminal (file only) — they were corrupting the readline prompt. The pattern list + the shared severity taxonomy (`operational`/`info`/`warn`/`error`/`critical`) live in `src/log-classify.ts` (`OPERATIONAL_PATTERNS`, `classifyLog`); `logger.ts:shouldSuppressLog` delegates to `isOperational()`. Add new noisy prefixes there, not in `logger.ts`.

² Non-`print` terminal output goes through `writeAbovePrompt` (readline-aware redraw) so async lines don't shred the input buffer; `console.warn`/`console.error` (and lines the taxonomy classifies `critical`) additionally call `setStatus` to mirror the latest notable event on the pinned **top alert bar** (`src/status-line.ts`). The always-on **bottom footer** (`src/footer.ts`) is the other pinned surface — both over one shared scroll region (`src/scroll-region.ts`). See the "Output surfaces" row of the doc-sync table.

³ `console.debug` routes to `logger.debug`, which is gated by `MIN_LOG_LEVEL` (`logger.ts`) — default `info`, so debug lines are **dropped entirely** unless `DEBUG=1`/`DEBUG=true` is set. Don't rely on debug output being in `cli.log` by default.

HoW/tradetunnel request diagnostics use this same existing console/logging
path in the CLI: with `DEBUG=1`/`DEBUG=true`, the shared
`@tealstreet/http-over-websocket` adapter writes sanitized `[TradeTunnel]`
request/response summaries through `console.log`, and `log-classify.ts` marks
that prefix operational so it lands in `~/.tealstreet/cli.log` without printing
above the prompt. There is no separate network-debug file. Headers are omitted,
sensitive keys are redacted, URLs keep query-key names but not query values,
and large payloads are truncated.

**Convention:** Use `logger.*` for operational details (file only). Use `print()` for user-facing status lines (terminal only).

**`logger.ts` must add no module-level timers at all.** `src/index.ts` imports
it at the top, so every subcommand pays for it: a ref'd `setInterval` here meant
`account add`, `exec` and friends printed their output, finished, and then hung
until the user hit Ctrl+C. Users read that as "the CLI never exits". The dedup
map is now evicted inside `isDuplicateMessage`, which already runs per line and
already reads the clock. `logger.event-loop.test.ts` is the gate — it counts
`process.getActiveResourcesInfo()` timers across the import and fails if the
count moves. See the root CLAUDE.md rule on `setInterval`.

## Commands

```bash
yarn dev           # Run with bun
yarn build         # Build cross-platform binaries (scripts/build.sh)
yarn release       # Bump + tag + push; tag fires .github/workflows/cli-release.yml (binaries, npm, homebrew).
                   # BRAND=disrupt yarn release:patch tags disrupt-cli-vX.Y.Z → DisruptTrade/cli, binaries only.
                   # Variants: yarn release:patch | :minor | :major.
                   # NO-ARG DOES NOT BUMP — release.sh only runs `yarn version` when
                   # given an argument, so a bare `yarn release` re-tags the current
                   # version and dies on "Tag already exists". Use :patch.
                   # Recovery if package.json was bumped without tagging:
                   #   git tag cli-v$(node -p "require('./package.json').version") master && git push origin --tags
yarn test          # Vitest
yarn logs          # Tail ~/.tealstreet/cli.log
```

## Key Documentation

- `README.md` — **dev-focused** (runtime, local dev, architecture pointers, terminal/watch-mode patterns, build & release). User-facing command/usage docs are NOT here — they live in `apps/documentation/cli/` → `docs.tealstreet.io/cli`.
- `ROADMAP.md` — Feature priorities

## Gotchas

- Uses **Bun** runtime, not Node.js — `bun-types` in tsconfig
- Watch mode terminal takeover: must recreate readline after raw mode (corrupts state)
- **One scroll region, two surfaces.** A terminal has a single DECSTBM
  scroll region. The top alert bar (`status-line.ts`) and the bottom
  footer (`footer.ts`) MUST NOT each call `ESC[...r` directly — they'd
  clobber each other. Both go through `scroll-region.ts`
  (`reserveTop`/`reserveBottom`), which computes the one combined region
  `[topReserved+1, rows-bottomReserved]` and owns the single resize +
  `process 'exit'` reset (a leaked region breaks the user's shell).
  Surfaces keep drawing their own CONTENT and register an
  `addResizeListener` callback (fired after the region is re-applied).
  Watch-mode alt-screen calls `suspend*`/`resume*` on BOTH surfaces; the
  coordinator's suspend is a shared flag, so order doesn't matter. The
  footer is **on by default** — it runs unless `config.footer.enabled` is
  explicitly `false` (the gate is `enabled !== false`, set via `footer
off`). `footer-segments.ts` is pure and unit-tested,
  the impure live snapshot (`buildFooterState`) lives in `repl.ts` and is
  injected via `setFooterRenderer`.
- **Footer `script` segments are hard read-only.** A `{type:'script',
name}` footer segment auto-runs the named script ~1×/sec
  (`runFooterScript` in `repl.ts`). Because of that cadence it runs with
  `FOOTER_READONLY_CAPS` (`writes: []`, no `runCommand`, no storage), so
  `capabilityCheck.ts` throws on any write/`runCommand` regardless of the
  script's own declared capabilities — a footer can **never** place or
  cancel orders. It also runs with a **silenced `CoreAdapter.print`** (only
  the return value is rendered, never terminal spam) and a **timeout +
  per-script in-flight guard** (a slow/hung script shows `!name` and never
  blocks the redraw or stacks runs). Don't loosen any of those three — an
  auto-running render surface is only safe because of them.
- **Feature-flag values from `/api/feature-flags` are mixed-type.** A flag value is now `boolean`
  OR a residual rule object (the server partial-evaluates flag rules; client-only-context leaves
  arrive unresolved). The CLI doesn't run the rule engine, so read a flag with a strict
  `flags?.FOO === true` (NOT `Boolean(flags?.FOO)` — a residual object is truthy).
  Engine: `@tealstreet/feature-flags-core/src/rules.ts`.
- Logger rate-limits to 5 msg/sec with 10s dedup window
- Exchange integration delegates to `@tealstreet/safe-cex`
- Build inlines env vars via `--define` (TEALSTREET_CLI_VERSION, URLs)
- **Preprocess pipeline.** `processCommand` in `repl.ts` runs every input
  through `preprocessCommand` from `cli-core` first: expands repeat blocks,
  splits the result on `;` and `&`, expands leading aliases, substitutes
  `@vars` and `$constants` (`$size`, `$entry`, …). `;` is **sequential**
  — the runner drains in-flight fire-and-forget steps before a
  `;`-separated step (its `awaitPrior` flag), so `close all; buy $1000`
  waits for the close — while `&` is **parallel**; `sleep`/`wait` always
  block (`block: true`). See the in-flight barrier in `processCommand`
  /`runStep` (TSBUG-106).
  The legacy `@cmd` parallel prefix is still accepted for backwards
  compat — stripped + warned at preprocess time.
- **Runner failure handling.** `cli-core.executeCommand` returns
  `success: false` / `error` for handled command failures (most
  commands return `this.error(...)` rather than throwing). `runStep`
  relies on `processSingleCommand` throwing on that shape so `retry`,
  `set ec`, blocking waits, and capture writeback all see failures.
  Do not collapse returned failures back into plain `{ handled: true }`.
  `command-outcome.ts` restores allowlisted `errorDetails` (including client IDs,
  unknown outcomes and partial accepted IDs) before the REPL throws. Its retry
  loop stops on unknown outcomes or accepted partial members; it must never
  reissue the whole write in those cases. Fully rejected operations retain the
  explicit retry count and the already-reported flag.
- **`FilePreprocessStore` (`src/preprocess-store.ts`).** Subclasses
  `MemoryPreprocessStore` from `cli-core` to persist vars/aliases to
  `~/.tealstreet/vars.json` + `~/.tealstreet/aliases.json` after every
  mutation (synchronous writes — a mid-session crash doesn't lose
  state). Constructor load paths must call `super` mutators via callbacks;
  going through overridden methods rewrites the persisted files on startup.
- **`export` / `import` REPL commands** live in
  `src/commands/export-import/` because they touch the filesystem.
  Distinct from `tealstreet account export/import` (top-level
  Commander route, dumps account credentials). Registered alongside
  the record commands at REPL start and at the top of
  `executeCommandWithCapture` so serve-mode invocations also see
  them.
- **REPL `tasks` / `kill` vs top-level `tealstreet tasks`.** REPL
  `tasks` (from cli-core) lists in-process chasers + TWAPs; top-level
  `tealstreet tasks list` reads `~/.tealstreet/tasks.json` via
  `apps/cli/src/tasks-store.ts`. Both intentional; do not collapse.
- **Write-op audit log (`src/audit-log.ts`, TSBUG-107).** Off by
  default; the `audit on/off/status/tail` REPL command is handled
  inline in `processSingleCommand` (not a cli-core command — it's
  CLI-only, no FS on web). `processSingleCommand` calls
  `recordWriteOp(command, account, result)` right after
  `cliExecuteCommand`, BEFORE the failure throw so failed writes are
  logged too. `recordWriteOp` (in `audit-log.ts`, where it's unit-
  tested) owns the gating: no-op unless audit on + `result.handled` +
  head in `AUDIT_WRITE_HEADS` (which includes aliases like `mv`/`lev`
  so the same op can't slip the audit). The log is strictly
  **append-only** — never rotate/rewrite it from the CLI; `readAuditTail`
  reads the whole file (fine for now; bounded read + rotation deferred).
  **Script-driven writes are also captured** (slice 2): `script:<name>`
  steps bypass `processSingleCommand`, so every write op is audited via
  the `onWrite` binding the CLI hands
  `@tealstreet/script-api-core`'s `createCoreAdapter`. That covers all
  nine writes — `placeOrder`/`placeOrders`/`cancelOrder`/`cancelOrders`/
  `updateOrder`/`closePosition`/`setLeverage`/`setPositionMode`/
  `setMarginType`; the last two were NOT audited while the adapter was
  hand-rolled. The observer fires from inside the shared body's own catch
  (success AND failure), which is what lets `closePosition` still report
  the ids that went live when a later leg throws. `runScriptStep` records the named
  invocation (against the override-resolved account) — all tagged
  `[script]`. **Command lineage (slice 3):** every entry carries a
  `runId` shared by all writes from one top-level invocation.
  `processCommand` wraps the whole chain loop in
  `runWithAuditRunId(newRunId(), …)`; the id is an `AsyncLocalStorage`
  value (dedicated store in `audit-log.ts`, separate from the
  adapter-context ALS), and `appendAuditEntry` defaults each entry's
  `runId` from `currentAuditRunId()`. Because it's ALS, the id flows into
  fire-and-forget `&`/default steps that settle _after_ `processCommand`
  returns, into a `repeat`'s expanded steps, and into a script's internal
  order writes (run inside the same scope) — no id is threaded through
  the adapter signatures. `audit tail` renders it as `run=<id>`.
  **Fill attribution (slice 4):** write ops that return order ids feed a
  bounded in-memory `orderId → {runId, command, account}` map
  (`rememberOrderLineage`, gated on audit-on so it can't grow unbounded,
  oldest evicted past `MAX_LINEAGE`). The exchange `fill` handler in
  `setupExchangeListeners` calls `recordFill(fill.orderId ?? fill.id, …)`;
  on a hit it appends a `[fill]` entry stamped with the lineage's `runId`
  (set explicitly — fills arrive async, outside the run scope) and a
  `← <originating command>` pointer. Unattributable fills (placed before
  audit was on / in a prior process) are skipped.
  **Rotation + bounded read (slice 5, closes TSBUG-107):** the live
  `audit.log` is size-capped (`maxLogBytes`, 5 MB) — tracked via an
  in-memory `logBytes` counter (lazily seeded from `statSync`, so no
  `stat` per write) and rolled to `audit.log.1` (single backup, replaced
  each time) once it's exceeded, so disk use is bounded at ~2× the cap.
  `readAuditTail` scans backwards from the end (`readTailLines`, 16 KB
  chunks, decoded once to avoid splitting a multi-byte char) and dips
  into `audit.log.1` only when a recent rotation left the live log short
  — it never slurps the whole file. Test hooks:
  `__setMaxLogBytesForTest`, and `__resetAuditCacheForTest` also clears
  `logBytes`.
- **ALS adapter context (`src/listener/adapter-context.ts`).**
  Thread-locals a `printSink` via `node:async_hooks`. The five
  `printX` methods on the standalone adapter consult `als.getStore()`
  first; when set, output routes to the sink (WS frame emit). Outside
  an ALS scope they fall through to terminal+log. **This is what lets
  `cmd:exec` and webhook fires stream without touching every
  `adapter.printX` call site in `repl.ts`** (~50+ of them). Late
  frames from fire-and-forget `;`/`&` steps propagate through `.catch`
  handlers attached during the scope — `cmd:done success:true` can be
  followed by additional `cmd:output kind:'error'` frames, by design.
  The same context **also carries `accountOverride` / `symbolOverride`**:
  when set (`cmd:exec` from a web tab with `account`/`symbol` fields),
  `processCommand` and `processSingleCommand` resolve `effectiveAccount`
  / `effectiveSymbol` / `effectiveExchange` from the override (looking
  up the exchange via `exchangeManager.getExchange`) instead of the
  long-lived REPL session. The `result.sessionChanged` writeback is
  also skipped on override runs so a web tab's `account <name>`
  doesn't hijack the REPL's current account. Webhook fires don't set
  overrides — they still run against the REPL session (see
  `apps/documentation/cli/operations/listener.md` → wire frames table for `mutate:account`).
- **`removeAccount` tombstones, doesn't physically drop.**
  `apps/cli/src/config.ts:removeAccount` sets `deletedAt + updatedAt`
  on the row rather than splicing it. `listAccounts()` and
  `getActiveAccounts()` filter `!deletedAt` so local UX is unchanged;
  the full snapshot path uses `listAllAccounts()`. Tombstones GC at
  listener startup if >30 days old (`gcTombstonedAccounts()`).
  **Don't add a code path that physically drops accounts** — the
  web side treats missing-from-snapshot as "ignore" (not "deleted"),
  so a physical drop would not propagate the delete and could
  silently desync.
- **Hosted webhooks run ONLY after the relay's ack.** `webhook-relay.ts`
  sends `webhook.ack received` and runs the body only on
  `webhook.acked accepted:true`; a refusal, a 10 s timeout or a dropped
  socket means it never runs here. That ack is the at-most-once guarantee
  (`fn_webhook_ack` in `supabase/webhook_relay.sql`), so do not add a path that
  runs first and acks after. The body goes onto the REPL's `commandQueue` (the
  same queue typed input uses) so an alert never interleaves with a chain the
  user is mid-way through. Started from `startRepl` only — one-shot commands
  must never hold that socket open. Design: `docs/plans/WEBHOOK_RELAY_v1.md`.
- **`webhookRefusal` (`packages/cli-core/src/webhook/webhookGuard.ts`) is the
  trust boundary for webhook bodies**, shared with the web listener. A hosted
  body is text from whoever holds the URL, so `runStep` refuses any step whose
  verb reconfigures the terminal or changes saved state when the ALS `source`
  is `'webhook'` (local hook fires share it). It resolves the verb through the
  command registry, so a command's aliases (`sim`, `use`) are refused with it,
  and it runs AFTER user-alias expansion. `account`/`symbol` are refused, so a
  webhook trades on the REPL's current account. A new command that changes
  credentials, access, safety limits, files or persistent state belongs on
  that list in the same commit.
- **A webhook run drains its whole chain before acking** (`processCommand(body,
  { drainFinalInFlight: true })`), or a one-step `buy` would report `done`
  before the order existed. A refused sign-in is never retried with the same
  token; the relay re-reads the token every minute so `login` revives it.
- **`X-Hook-Token` length-guard.**
  `apps/cli/src/listener/hooks-store.ts:verifyHookToken`
  short-circuits on `token.length !== hook.secret.length` BEFORE
  `crypto.timingSafeEqual`. Don't remove the guard —
  `timingSafeEqual` throws `RangeError` on unequal-length buffers,
  which would surface as HTTP 500 on an empty-token POST instead of
  a clean 401.
- **Phase 6 types bundle — now in `packages/script-authoring/src/`.**
  `script-types-bundle.ts`, its drift test, and `scripts-templates.ts` moved
  out of `apps/cli/src/` because `apps/cli` and `@tealstreet/script-authoring`
  had become a turbo dependency CYCLE, which broke `yarn build` for the entire
  repo. A package may not depend on an app; the shared constants had to live in
  the package. `apps/cli` imports them via
  `@tealstreet/script-authoring/src/...`. Generated
  artifact, committed. Pipeline: `yarn generate:types` at the repo
  root runs `scripts/generate-script-types-bundle.ts`, which calls
  the three per-package `scripts/generate-monaco-types.ts` (core /
  cli / web) and writes the assembled bundle file. The bundle is
  `import`ed by that package's `scripts-templates.ts` to build `TYPES_TEMPLATE`,
  which `scripts-store.ts:ensureScriptsDir` writes to
  `~/.tealstreet/scripts/types.d.ts` on every launch. Drift safety:
  `packages/script-authoring/src/script-types-bundle.test.ts` re-runs the orchestrator in-memory
  and compares against the on-disk file — CI fails if the two
  diverge, forcing `yarn generate:types` + commit. **Never
  hand-edit `script-types-bundle.ts`** — fix the generators or the
  source packages and regenerate. The core generator uses a
  dedicated `packages/script-api-core/scripts/bundle.tsconfig.json`
  that carries the workspace `paths` + DOM lib inline so
  `dts-bundle-generator` can resolve `@tealstreet/*` and safe-cex's
  DOM-touching utils (CompressionFormat, BufferSource) without
  per-package include scopes.
- **Phase 6 v1.1 entry-point split (`packages/script-api-core/src/script.ts`).**
  The Phase 6 generator now walks `src/script.ts` (user-facing
  surface only), NOT `src/index.ts`. The package's `main` is still
  `index.ts` so every host consumer (CLI's `script-runner.ts`,
  `scripts-store.ts`, the web worker shim, all tests) keeps
  importing the full surface from `@tealstreet/script-api-core`
  unchanged — only the bundle target changed. The seeded
  `types.d.ts` no longer contains `InProcessProxy`, `runScript`,
  `buildApi`, capability validators, transpile helpers, etc.
  **Leak guard**: `packages/script-authoring/src/script-types-bundle.test.ts` runs 32 regex
  assertions against `CORE_TYPES_DTS` to fail CI if any host symbol
  re-appears as an actual TS declaration (textual mentions inside
  JSDoc comments are allowed — they aren't load-bearing). When
  adding a new host symbol to `index.ts`, add it to
  `HOST_ONLY_SYMBOLS` in the same PR.
- **Drift block skips in the OSS mirror — don't "fix" the glob import.**
  `packages/script-authoring/src/script-types-bundle.test.ts`'s drift block re-runs
  `scripts/generate-script-types-bundle.ts`, a monorepo-root script that
  is **not** in the `tealstreet-sdk` mirror slice. The block loads the
  generator via a Vite partial-dynamic specifier
  (``import(`../../../scripts/${generatorName}.ts`)``): the glob matches
  in the monorepo (gate runs, 0 skipped) and matches nothing in the mirror,
  where the dir is absent (import rejects → `describe.skipIf` skips). The
  template specifier also stops tsc binding the missing module, keeping the
  mirror's typecheck clean. Do not convert it back to a static/literal
  import — that breaks mirror typecheck (`TS2307`). The host-symbol leak
  guard in the same file reads the committed in-slice bundle and runs in
  both. (Authoritative drift gate stays in the monorepo on sync-back —
  `docs/safe-cex-cli-mirror-plan.md` §9.)
- **Script `runCommand` intentionally fails unhandled commands.**
  `src/script-adapter.ts:createStandaloneCoreAdapter().runCommand`
  calls `cli-core.executeCommand` directly, not `processCommand` /
  `runStep`. That means it does not run the preprocess pipeline, does
  not see CLI-only commands handled in `repl.ts` (`script reload`,
  `script:<name>`, hooks, logs, watch modes, record/export/import
  registration side effects), and does not process REPL-only
  `taskAction` follow-ups like TWAP startup. Because this is a narrower
  surface than the REPL, `{ handled: false }` from `executeCommand`
  must map to `RunCommandResult.ok === false` (`Unknown command: ...`),
  never silent success. If you later widen `runCommand` to route
  through `processCommand`, keep an explicit deny story for management
  commands (`script reload`, `hook create`, `clients revoke`, watch
  modes, etc.).
- **Script `getOrderbook` is subscription-backed, not best-effort.**
  `src/script-orderbook-cache.ts` subscribes through the connected
  exchange's real `listenOrderBook`, waits briefly for a snapshot, and
  rebuilds if reconnect swaps the account exchange object. Keep
  `api.getHistoricalPosition` out of the CLI target descriptor: web/mobile
  read an app position-history store, while CLI has no equivalent store.
  An on-demand `fetchHistoricalFills` call would be a thinner,
  venue-limited contract, not parity.
- **Script retry idempotency is checked before the first attempt.**
  `retry N script:<name>` must refuse a non-idempotent script before
  user code runs at all. `runScriptStep` performs this preflight; the
  lower-level `script-runner.ts` gate still protects direct host calls
  where `retry: { isRetry: true }` is supplied.
