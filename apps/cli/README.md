# Tealstreet CLI

Trade crypto from your terminal. Built on **Bun**, compiled to standalone
cross-platform binaries (darwin-arm64/x64, linux-x64/arm64).

> **Looking for usage docs?** Commands, account setup, sizing/price grammar,
> workflow operators, and recipes live at **<https://docs.tealstreet.io/cli/>**
> (source in [`apps/documentation/cli/`](../documentation/cli/)). **This README
> is for working on the CLI itself** — see that site for anything user-facing.

While the REPL is running it also acts as a loopback side-car for the web app
on the same machine — see
[`operations/listener.md`](../documentation/cli/operations/listener.md) for the
listener wire protocol, auth, and kill switches.

## Runtime

**Bun**, not Node.js (`bun-types` in tsconfig). Binaries are produced with
`bun build --compile`. Exchange integration delegates to
[`@tealstreet/safe-cex`](../../packages/safe-cex); shared command logic lives in
[`@tealstreet/cli-core`](../../packages/cli-core).

## Develop locally

Prerequisite: [Bun](https://bun.sh/) — the CLI's scripts invoke `bun`
directly. Install workspace deps from the **monorepo root** first (this is a
yarn workspace; `@tealstreet/*` deps resolve from there).

```bash
yarn install                 # from the monorepo root
corepack yarn -s rust:build-wasm # build the Rust engine asset before source runs/builds

cd apps/cli
yarn dev                     # run the REPL from source (= bun run src/index.ts)
yarn typecheck               # tsc --noEmit
yarn test                    # vitest
yarn logs                    # tail ~/.tealstreet/cli.log
```

### Build a local binary

`yarn build` compiles standalone binaries for all four platforms (via
`bun build --compile`) into `dist/tealstreet-<platform>`. To build and run the
one for your machine:

```bash
yarn build
./dist/tealstreet-darwin-arm64        # or -darwin-x64 / -linux-x64 / -linux-arm64
```

The same tree also builds the Disrupt-branded CLI (`src/brand.ts`): `yarn build
disrupt` emits `dist/disrupt-<platform>`, logging in through `app.disrupt.trade` and
storing under `~/.disrupt/`.

### npm package

The public `@tealstreet/cli` package is a thin installer/launcher. It does not
ship CLI source; it downloads the matching binary from
`https://github.com/Tealstreet/cli/releases` on install or first run.

Config and runtime state live under `~/.tealstreet/` — see
[`operations/config-files.md`](../documentation/cli/operations/config-files.md).

## Architecture

[`CLAUDE.md`](./CLAUDE.md) has the full source map, the listener subsystem, the
remote-signing model, the logging taxonomy, and the script-types pipeline.
High-level entry points:

- `src/index.ts` — Commander.js entry
- `src/repl.ts` — interactive REPL (readline)
- `src/listener/` — loopback `127.0.0.1:53219` WS + HTTP side-car for the web app
- `src/commands/` — command implementations (shared logic in `@tealstreet/cli-core`)
- `src/config.ts` — `~/.tealstreet/config.json` (accounts, listener kill-switches, footer layout)
- `src/logger.ts` + `src/log-classify.ts` — console interception, severity taxonomy, operational-noise routing
- `src/scroll-region.ts` + `src/status-line.ts` + `src/footer.ts` + `src/footer-segments.ts` — the pinned output surfaces (top alert bar + bottom status footer over one shared ANSI scroll region)
- `src/audit-log.ts` — durable append-only write-op trail + command tracing (`audit` command)

Account add/import surfaces must use
`@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig`; do not add
CLI-local credential schemas or exchange support tables.

## Development

### Watch Mode Pattern (Terminal Takeover)

Watch modes (wa, wp, wo, wm, watch chart, watch chase) need to "take over" the terminal to:

1. Display live-updating content
2. Capture single keystrokes to exit (Escape or Ctrl+X in REPL mode, also Ctrl+C when running via CLI args)
3. Prevent the REPL prompt from showing

**Exit Keys** (centralized in `utils/exit-keys.ts`):

- **REPL mode**: Escape or Ctrl+X (Ctrl+C stays with readline for line cancel)
- **CLI args mode**: Ctrl+C, Escape, or Ctrl+X

**The Pattern** (see `startWatchAccount` in repl.ts):

```typescript
import { isReplExitKey, getReplExitMessage } from './utils/exit-keys';

function startWatchMode(rl: readline.Interface): void {
  // 1. Set isMonitorMode flag - CRITICAL! This prevents REPL prompt from showing
  isMonitorMode = true;

  // 2. Pause readline to stop it from processing input
  rl.pause();

  // 3. Enter alternate screen buffer (optional - keeps scrollback clean)
  process.stdout.write(ENTER_ALT_SCREEN);

  // 4. Set up raw mode key handler for exit keys
  const onRawKey = (data: Buffer) => {
    const key = data[0];
    if (isReplExitKey(key)) {
      stopWatch();
    }
  };

  const stopWatch = () => {
    if (!isMonitorMode) return; // Prevent double-stop
    isMonitorMode = false;

    // Clean up: remove key handler, exit raw mode, exit alt screen
    process.stdin.removeListener('data', onRawKey);
    if (process.stdin.isTTY) process.stdin.setRawMode(false);
    process.stdout.write(EXIT_ALT_SCREEN);

    // CRITICAL: Recreate readline to restore terminal state
    recreateReadline();
  };

  // 5. Store stop function for cleanup (used by readline close handler)
  stopMonitorFn = stopWatch;

  // 6. Enter raw mode and start listening
  if (process.stdin.isTTY) process.stdin.setRawMode(true);
  process.stdin.on('data', onRawKey);

  // 7. Return immediately! The function does NOT block.
  // Watch mode runs via setInterval
}
```

**Why isMonitorMode is CRITICAL**:

- The REPL's command handler always shows the prompt after a command completes
- `isMonitorMode = true` suppresses this behavior (checked before calling `rl.prompt()`)
- Without this flag, the REPL prompt appears immediately after the watch command "returns"

**Key Points**:

1. Watch functions return IMMEDIATELY - they don't await/block
2. `isMonitorMode = true` prevents REPL prompt from showing
3. Raw mode (`setRawMode(true)`) captures single keystrokes
4. Must call `recreateReadline()` on cleanup (raw mode corrupts readline state)

### Raw Mode and Readline Pattern

Watch modes use raw stdin mode (`setRawMode(true)`) to capture single keystrokes (Escape, Ctrl+X). This corrupts Node.js readline's internal state, causing issues like:

- Double-printed characters
- Arrow keys printing escape sequences (`[A`) instead of navigating history
- Random input clearing

**Solution**: After exiting raw mode, **recreate the readline interface entirely** instead of just resuming it. The `recreateReadline()` helper in `repl.ts` handles this:

```typescript
function recreateReadline(): void {
  if (!rlInstance) return;

  // Preserve history
  const history = (rlInstance as readline.Interface & { history?: string[] }).history || [];

  // Close old readline (with flag to prevent exit handler)
  isRecreatingReadline = true;
  rlInstance.close();
  isRecreatingReadline = false;

  // Create fresh readline
  rlInstance = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: getPrompt(),
    history: [...history],
    historySize: MAX_HISTORY_SIZE,
  });

  // Re-setup event handlers...
}
```

**Important**: All watch mode stop functions must call `recreateReadline()` to restore proper terminal state.

### Console output, logging & status surfaces

`src/logger.ts` intercepts `console.*` and routes each line through, in order:
**operational-noise suppression** → **rate limit** → **readline-aware
printing**. The full taxonomy and the user-facing behaviour are documented in
[`CLAUDE.md`](./CLAUDE.md#logging) and the public
[Output reference](../documentation/cli/reference/output.md) — keep all three in
sync (see the doc-sync table at the top of `CLAUDE.md`). The essentials:

- **Operational noise → file only.** Lines matching `OPERATIONAL_PATTERNS` in
  `src/log-classify.ts` (keepalive pings, reconnect chatter, balance dumps) are
  kept out of the terminal — they still hit `~/.tealstreet/cli.log`. Add new
  noisy prefixes there, not in `logger.ts`.
- **Everything else prints _above_ the prompt** via `writeAbovePrompt`
  (readline-aware), so async output never corrupts what the user is typing.
  `console.warn` / `console.error` (and any line `log-classify.ts` rates
  `critical`) are additionally mirrored on the **top status bar**
  (`src/status-line.ts`). ("critical" is a severity classification, not a
  `console` method.)
- **`print()` / `originalConsoleLog()` bypass all of the above** — use them for
  primary command output, prompts, progress, and anything the user explicitly
  asked for.
- **Verbose transport diagnostics share `cli.log`.** When the CLI runs with
  `DEBUG=1` or `DEBUG=true`, tunneled HoW/tradetunnel REST requests emit
  sanitized `[TradeTunnel]` request/response summaries through the existing
  `console.log` interception. The `[TradeTunnel]` prefix is classified as
  operational, so those lines land in `~/.tealstreet/cli.log` and stay off the
  terminal. This is off by default; it is not a separate network log file.

```typescript
import { originalConsoleLog } from '../logger';
const print = originalConsoleLog; // never throttled / suppressed
print(chalk.green('✓ Operation complete'));
```

**Pinned surfaces** (one shared ANSI scroll region, `src/scroll-region.ts`):
the transient **top alert bar** (`status-line.ts`, warn/error/critical, dims)
and the always-on **bottom footer** (`footer.ts` + `footer-segments.ts`,
user-customizable segments). Both no-op on non-TTY.

**Command tracing**: write-ops can be recorded to a durable append-only
`~/.tealstreet/audit.log` with per-invocation `runId` + fill attribution
(`src/audit-log.ts`, `audit` command).

## Building & Releasing

### Prerequisites

- [Bun](https://bun.sh/) - `curl -fsSL https://bun.sh/install | bash`

### Build (local only)

```bash
# Build binaries for all platforms (darwin-arm64, darwin-x64, linux-x64, linux-arm64)
bun run build
```

Binaries are output to `dist/`.

### Release to GitHub

```bash
# Release with version bump
yarn release:patch     # 0.1.0 -> 0.1.1
yarn release:minor     # 0.1.0 -> 0.2.0
yarn release:major     # 0.1.0 -> 1.0.0

# Release current package.json version
yarn release
```

This will:

1. Bump version in package.json (if specified)
2. Commit the version bump
3. Push a private monorepo tag named `cli-vX.Y.Z`
4. Let GitHub Actions build binaries, publish public `Tealstreet/cli`
   release `vX.Y.Z`, and update `Tealstreet/homebrew-tap`

The workflow uses repository secret `TEALSTREET_CLI_RELEASE_TOKEN` to write
to the public release repo and Homebrew tap.

## License

MIT
