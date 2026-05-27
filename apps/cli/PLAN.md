# Tealstreet CLI - Implementation Plan

> **Status: historical reference only — see `RELEASE_NOTES.md` and the code under `apps/cli/` for current shipped behavior.**

## Overview

Create a standalone CLI tool that reuses the existing command system from `@tealstreet/safe-cex`. The CLI will work both as a module in the web app AND as an independent installable tool distributed via Homebrew, apt, and direct download.

## Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        safe-cex package                          │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  commands/router.ts - Command parsing & execution       │    │
│  │  commands/BuyCommand.ts, SellCommand.ts, etc.           │    │
│  │  exchanges/base.ts - Exchange base class                │    │
│  │  exchanges/bybit/, binance/, hyperliquid/, etc.         │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┴───────────────┐
              │                               │
              ▼                               ▼
┌─────────────────────────┐     ┌─────────────────────────┐
│    Web App (React)      │     │   Standalone CLI (Bun)  │
│  ┌───────────────────┐  │     │  ┌───────────────────┐  │
│  │ CLI.tsx           │  │     │  │ index.ts          │  │
│  │ crt-terminal UI   │  │     │  │ readline/ink UI   │  │
│  │ Jotai state       │  │     │  │ File-based config │  │
│  └───────────────────┘  │     │  └───────────────────┘  │
└─────────────────────────┘     └─────────────────────────┘
```

## Directory Structure

```
apps/cli/
├── package.json
├── tsconfig.json
├── README.md
├── src/
│   ├── index.ts              # Entry point - argument parsing
│   ├── repl.ts               # Interactive REPL mode
│   ├── config.ts             # Config file management (~/.tealstreet/)
│   ├── exchange-factory.ts   # Create exchange instances for CLI
│   ├── output.ts             # Terminal output formatting
│   └── commands/
│       └── config-commands.ts # CLI-specific commands (config, accounts)
├── scripts/
│   └── build.sh              # Build script for all platforms
└── .github/
    └── workflows/
        └── release.yml       # CI/CD for building & releasing
```

## Implementation Steps

### Phase 1: Basic CLI Structure

#### 1.1 Create package.json

```json
{
  "name": "@tealstreet/cli",
  "version": "0.1.0",
  "description": "Tealstreet CLI - Trade crypto from your terminal",
  "main": "src/index.ts",
  "bin": {
    "tealstreet": "src/index.ts"
  },
  "scripts": {
    "dev": "bun run src/index.ts",
    "build": "bun build src/index.ts --compile --outfile dist/tealstreet",
    "build:all": "./scripts/build.sh"
  },
  "dependencies": {
    "@tealstreet/safe-cex": "workspace:*",
    "commander": "^12.0.0",
    "chalk": "^5.3.0",
    "ora": "^8.0.0",
    "inquirer": "^9.0.0"
  }
}
```

#### 1.2 Create src/index.ts - Entry Point

```typescript
#!/usr/bin/env bun
import { Command } from 'commander';
import { startRepl } from './repl';
import { loadConfig, saveConfig } from './config';
import { executeCommand } from './execute';

const program = new Command();

program.name('tealstreet').description('Trade crypto from your terminal').version('0.1.0');

// Config commands
program
  .command('config')
  .description('Configure API keys and settings')
  .option('-e, --exchange <exchange>', 'Exchange name (bybit, binance, hyperliquid, etc.)')
  .option('-k, --api-key <key>', 'API key')
  .option('-s, --api-secret <secret>', 'API secret')
  .option('-l, --list', 'List configured accounts')
  .action(handleConfig);

// Direct command execution
program
  .command('exec <command...>')
  .description('Execute a trading command')
  .option('-a, --account <account>', 'Account to use')
  .option('-s, --symbol <symbol>', 'Trading symbol')
  .action(handleExec);

// Interactive mode (default)
program.command('interactive', { isDefault: true }).description('Start interactive REPL mode').action(startRepl);

// Shorthand commands that pass through to safe-cex
const tradingCommands = ['buy', 'sell', 'close', 'cancel', 'scale', 'chase', 'nuke', 'balance'];
tradingCommands.forEach((cmd) => {
  program
    .command(`${cmd} [args...]`)
    .description(`Execute ${cmd} command`)
    .action((args) => executeCommand(`${cmd} ${args.join(' ')}`));
});

program.parse();
```

#### 1.3 Create src/config.ts - Configuration Management

```typescript
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'fs';
import { homedir } from 'os';
import { join } from 'path';

const CONFIG_DIR = join(homedir(), '.tealstreet');
const CONFIG_FILE = join(CONFIG_DIR, 'config.json');

export interface AccountConfig {
  name: string;
  exchange: string;
  apiKey: string;
  apiSecret: string;
  passphrase?: string; // For some exchanges
  testnet?: boolean;
}

export interface Config {
  defaultAccount?: string;
  defaultSymbol?: string;
  accounts: AccountConfig[];
}

export function loadConfig(): Config {
  if (!existsSync(CONFIG_DIR)) {
    mkdirSync(CONFIG_DIR, { recursive: true });
  }

  if (!existsSync(CONFIG_FILE)) {
    return { accounts: [] };
  }

  return JSON.parse(readFileSync(CONFIG_FILE, 'utf-8'));
}

export function saveConfig(config: Config): void {
  if (!existsSync(CONFIG_DIR)) {
    mkdirSync(CONFIG_DIR, { recursive: true });
  }
  writeFileSync(CONFIG_FILE, JSON.stringify(config, null, 2));
}

export function getAccount(name?: string): AccountConfig | undefined {
  const config = loadConfig();
  if (name) {
    return config.accounts.find((a) => a.name === name);
  }
  return config.accounts.find((a) => a.name === config.defaultAccount) || config.accounts[0];
}
```

#### 1.4 Create src/exchange-factory.ts - Exchange Initialization

```typescript
import { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';
import { createExchange } from '@tealstreet/safe-cex';
import { AccountConfig } from './config';

// Map of exchange name to exchange class
const EXCHANGE_MAP: Record<string, string> = {
  bybit: 'bybit',
  binance: 'binance',
  'binance-futures': 'binance-futures',
  hyperliquid: 'hyperliquid',
  blofin: 'blofin',
  woo: 'woo',
  bitget: 'bitget',
  // Add more as needed
};

export async function createExchangeFromConfig(account: AccountConfig): Promise<Exchange> {
  const exchangeType = EXCHANGE_MAP[account.exchange.toLowerCase()];
  if (!exchangeType) {
    throw new Error(`Unknown exchange: ${account.exchange}`);
  }

  const exchange = createExchange(exchangeType, {
    apiKey: account.apiKey,
    apiSecret: account.apiSecret,
    passphrase: account.passphrase,
    testnet: account.testnet,
  });

  // Start the exchange (load markets, connect websockets)
  await exchange.start();

  return exchange;
}
```

#### 1.5 Create src/repl.ts - Interactive Mode

```typescript
import * as readline from 'readline';
import chalk from 'chalk';
import { loadConfig, getAccount } from './config';
import { createExchangeFromConfig } from './exchange-factory';
import { commandRouter } from '@tealstreet/safe-cex/src/webworker/commands/router';
import { Exchange } from '@tealstreet/safe-cex/src/exchanges/base';

let currentExchange: Exchange | null = null;
let currentSymbol = 'BTCUSDT';
let currentAccount: string | undefined;

export async function startRepl() {
  const config = loadConfig();

  if (config.accounts.length === 0) {
    console.log(chalk.yellow('No accounts configured. Run: tealstreet config --help'));
    process.exit(1);
  }

  const account = getAccount();
  if (!account) {
    console.log(chalk.red('No account found'));
    process.exit(1);
  }

  console.log(chalk.cyan(`Connecting to ${account.exchange}...`));

  try {
    currentExchange = await createExchangeFromConfig(account);
    currentAccount = account.name;
  } catch (error) {
    console.log(chalk.red(`Failed to connect: ${error.message}`));
    process.exit(1);
  }

  // Listen for command output
  currentExchange.emitter.on('exec-log', ({ message }) => {
    console.log(message);
  });

  console.log(chalk.green(`Connected to ${account.exchange}`));
  console.log(chalk.gray(`Symbol: ${currentSymbol} | Account: ${currentAccount}`));
  console.log(chalk.gray('Type "help" for available commands, "exit" to quit\n'));

  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
    prompt: chalk.cyan('tealstreet> '),
  });

  rl.prompt();

  rl.on('line', async (line) => {
    const command = line.trim();

    if (!command) {
      rl.prompt();
      return;
    }

    if (command === 'exit' || command === 'quit') {
      console.log(chalk.gray('Goodbye!'));
      await currentExchange?.stop();
      process.exit(0);
    }

    // Handle symbol switching
    if (command.startsWith('symbol ')) {
      currentSymbol = command.split(' ')[1].toUpperCase();
      console.log(chalk.gray(`Symbol set to ${currentSymbol}`));
      rl.prompt();
      return;
    }

    // Execute command via safe-cex router
    try {
      await commandRouter(
        currentExchange!,
        {} as any, // settings - TODO: implement
        {} as any, // metadata - TODO: implement
        command,
        currentSymbol,
      );
    } catch (error) {
      console.log(chalk.red(`Error: ${error.message}`));
    }

    rl.prompt();
  });

  rl.on('close', async () => {
    await currentExchange?.stop();
    process.exit(0);
  });
}
```

### Phase 2: Build System

#### 2.1 Create scripts/build.sh

```bash
#!/bin/bash
set -e

VERSION=${1:-"0.1.0"}
DIST_DIR="dist"

echo "Building Tealstreet CLI v${VERSION}..."

# Clean
rm -rf $DIST_DIR
mkdir -p $DIST_DIR

# Build for each platform
PLATFORMS=(
  "darwin-arm64"
  "darwin-x64"
  "linux-x64"
  "linux-arm64"
  "windows-x64"
)

for PLATFORM in "${PLATFORMS[@]}"; do
  echo "Building for ${PLATFORM}..."

  TARGET=""
  EXT=""
  case $PLATFORM in
    darwin-arm64) TARGET="bun-darwin-arm64" ;;
    darwin-x64) TARGET="bun-darwin-x64" ;;
    linux-x64) TARGET="bun-linux-x64" ;;
    linux-arm64) TARGET="bun-linux-arm64" ;;
    windows-x64) TARGET="bun-windows-x64"; EXT=".exe" ;;
  esac

  OUTPUT="${DIST_DIR}/tealstreet-${PLATFORM}${EXT}"

  bun build src/index.ts \
    --compile \
    --target=$TARGET \
    --outfile=$OUTPUT

  # Create tarball (except Windows)
  if [ "$PLATFORM" != "windows-x64" ]; then
    tar -czvf "${DIST_DIR}/tealstreet-${VERSION}-${PLATFORM}.tar.gz" -C $DIST_DIR "tealstreet-${PLATFORM}"
  else
    (cd $DIST_DIR && zip "tealstreet-${VERSION}-${PLATFORM}.zip" "tealstreet-${PLATFORM}.exe")
  fi
done

echo "Build complete!"
ls -la $DIST_DIR
```

#### 2.2 Create .github/workflows/release.yml

```yaml
name: Release CLI

on:
  push:
    tags:
      - 'cli-v*'

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: oven-sh/setup-bun@v1
        with:
          bun-version: latest

      - name: Install dependencies
        run: bun install
        working-directory: apps/cli

      - name: Build all platforms
        run: ./scripts/build.sh ${{ github.ref_name }}
        working-directory: apps/cli

      - name: Create Release
        uses: softprops/action-gh-release@v1
        with:
          files: apps/cli/dist/*.tar.gz,apps/cli/dist/*.zip
          generate_release_notes: true

  update-homebrew:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - name: Update Homebrew formula
        uses: mislav/bump-homebrew-formula-action@v3
        with:
          formula-name: tealstreet-cli
          homebrew-tap: tealstreet/homebrew-tap
          download-url: https://github.com/Tealstreet/tealstreet-next/releases/download/${{ github.ref_name }}/tealstreet-${{ github.ref_name }}-darwin-arm64.tar.gz
        env:
          COMMITTER_TOKEN: ${{ secrets.HOMEBREW_TAP_TOKEN }}
```

> _The `update-homebrew` GitHub Actions job above and the Phase 3
> "Create Homebrew Tap Repository" / "Create Formula" sub-plans below
> have **shipped** — see `apps/cli/scripts/release.sh` (which builds,
> uploads, edits `apps/cli/homebrew/tealstreet.rb`, and pushes to
> `Tealstreet/homebrew-tap` as `Formula/tealstreet.rb`)._

### ~~Phase 3: Homebrew Distribution~~ (shipped)

#### ~~3.1 Create Homebrew Tap Repository~~ (shipped — `Tealstreet/homebrew-tap`)

Create a new repo: `github.com/tealstreet/homebrew-tap`

#### ~~3.2 Create Formula: Formula/tealstreet-cli.rb~~ (shipped — see `apps/cli/homebrew/tealstreet.rb`; class is `Tealstreet`, not `TealstreetCli`)

```ruby
class TealstreetCli < Formula
  desc "Trade crypto from your terminal"
  homepage "https://tealstreet.io"
  version "0.1.0"
  license "MIT"

  on_macos do
    on_arm do
      url "https://github.com/Tealstreet/tealstreet-next/releases/download/cli-v0.1.0/tealstreet-0.1.0-darwin-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"

      def install
        bin.install "tealstreet-darwin-arm64" => "tealstreet"
      end
    end
    on_intel do
      url "https://github.com/Tealstreet/tealstreet-next/releases/download/cli-v0.1.0/tealstreet-0.1.0-darwin-x64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"

      def install
        bin.install "tealstreet-darwin-x64" => "tealstreet"
      end
    end
  end

  on_linux do
    on_arm do
      url "https://github.com/Tealstreet/tealstreet-next/releases/download/cli-v0.1.0/tealstreet-0.1.0-linux-arm64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"

      def install
        bin.install "tealstreet-linux-arm64" => "tealstreet"
      end
    end
    on_intel do
      url "https://github.com/Tealstreet/tealstreet-next/releases/download/cli-v0.1.0/tealstreet-0.1.0-linux-x64.tar.gz"
      sha256 "PLACEHOLDER_SHA256"

      def install
        bin.install "tealstreet-linux-x64" => "tealstreet"
      end
    end
  end

  test do
    assert_match "tealstreet", shell_output("#{bin}/tealstreet --version")
  end
end
```

### Phase 4: Documentation

#### 4.1 Create apps/cli/README.md

````markdown
# Tealstreet CLI

Trade crypto from your terminal.

## Installation

### Homebrew (macOS/Linux)

```bash
brew install tealstreet/tap/tealstreet-cli
```
````

### Direct Download

Download from [Releases](https://github.com/Tealstreet/tealstreet-next/releases)

### From Source

```bash
git clone https://github.com/Tealstreet/tealstreet-next
cd tealstreet-next/apps/cli
bun install
bun run build
```

## Configuration

First, configure your API keys:

```bash
tealstreet config -e bybit -k YOUR_API_KEY -s YOUR_API_SECRET
```

List configured accounts:

```bash
tealstreet config -l
```

## Usage

### Interactive Mode

```bash
tealstreet
> buy $100
> sell ETHUSDT $50 reduce
> cancel all
> help
```

### One-off Commands

```bash
tealstreet buy BTCUSDT $100
tealstreet sell $50 reduce
tealstreet close 50%
tealstreet cancel all
tealstreet balance
```

## Commands

| Command   | Description            | Example                                |
| --------- | ---------------------- | -------------------------------------- |
| `buy`     | Buy market/limit       | `buy $100`, `buy ETHUSDT $100 at 3000` |
| `sell`    | Sell market/limit      | `sell $100`, `sell $100 reduce`        |
| `close`   | Close position         | `close 50%`, `close all`               |
| `cancel`  | Cancel orders          | `cancel all`, `cancel buys`            |
| `scale`   | Scale into position    | `scale $1000 5 orders`                 |
| `chase`   | Chase order            | `chase buy $100`                       |
| `nuke`    | Cancel all & close all | `nuke`                                 |
| `balance` | Show balance           | `balance`                              |
| `help`    | Show help              | `help buy`                             |

## Supported Exchanges

- Bybit
- Binance Futures
- Hyperliquid
- BloFin
- WOO X
- Bitget
- More coming...

## License

MIT

```

## Dependencies Analysis

### What safe-cex needs to work standalone:

1. **Network**: `axios` for REST, `ws` for WebSocket - both work in Node/Bun
2. **Crypto**: Node.js crypto module - available in Bun
3. **Events**: Node.js EventEmitter - available in Bun
4. **No browser APIs**: safe-cex doesn't use DOM, localStorage, etc.

### Potential Issues to Resolve:

1. **WebWorker usage**: The web app runs safe-cex in a WebWorker. For CLI, we run it directly in the main thread.
2. **Settings/Metadata types**: Need to provide mock or simplified versions for CLI use.
3. **Market data loading**: May need to wait for markets to load before executing commands.

## Timeline Estimate

| Phase | Tasks | Complexity |
|-------|-------|------------|
| Phase 1 | Basic CLI structure | Medium |
| Phase 2 | Build system | Low |
| Phase 3 | Homebrew setup | Low |
| Phase 4 | Documentation | Low |
| Testing | Cross-platform testing | Medium |

## Future Enhancements

1. **TUI Mode**: Full terminal UI with positions, orders, charts using `blessed` or `ink`
2. **Scripting**: Load and execute command files
3. **Alerts**: Price alerts that run in background
4. **Multiple accounts**: Easy account switching
5. **Auto-completion**: Tab completion for symbols and commands
6. **History**: Persistent command history
7. **Aliases**: Custom command aliases
```
