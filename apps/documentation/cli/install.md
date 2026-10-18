---
title: Install
sidebar_position: 1
---

# Install

Standalone binaries are published to
[`Tealstreet/cli`](https://github.com/Tealstreet/cli/releases). Pick a
channel below; both install the same artifact.

## macOS & Linux

```bash
# One-liner — detects platform/arch, drops into /usr/local/bin (asks for sudo if it is not writable)
curl -fsSL https://raw.githubusercontent.com/Tealstreet/cli/main/install.sh | bash

# Homebrew tap
brew install tealstreet/tap/tealstreet
```

Verify:

```bash
tealstreet --version
```

If you see `command not found`, `/usr/local/bin` isn't on your `PATH` —
add it, or symlink the binary somewhere that is.

The Homebrew formula lives in
[`Tealstreet/homebrew-tap`](https://github.com/Tealstreet/homebrew-tap).

## Windows

```powershell
npm install -g @tealstreet/cli
```

That fetches `tealstreet-windows-x64.exe` and puts a `tealstreet` command on
your PATH. For a direct download, grab the `.exe` from the releases page and
put it somewhere on your PATH yourself.

Two things to know:

- **Run it from Windows Terminal or PowerShell 7.** The REPL draws a pinned
  top bar and bottom footer using ANSI escape sequences. Legacy `cmd.exe`
  does not render those correctly.
- **`tealstreet update` works either way.** An npm install upgrades through
  `npm update -g`. A directly-downloaded `.exe` is replaced by renaming the
  running binary aside and moving the new one into place — Windows will not
  let a running `.exe` be overwritten, only renamed. Restart the CLI
  afterwards; the leftover `.old` file is removed on the next launch.

WSL remains a fine alternative — the `linux-x64` binary runs there with the
macOS/Linux one-liner above.

## Available builds

| Asset                        | Platform           |
| ---------------------------- | ------------------ |
| `tealstreet-darwin-arm64`    | Apple Silicon Macs |
| `tealstreet-darwin-x64`      | Intel Macs         |
| `tealstreet-linux-x64`       | Linux x86_64       |
| `tealstreet-linux-arm64`     | Linux arm64        |
| `tealstreet-windows-x64.exe` | Windows x64        |

---

## First run

```bash
tealstreet
```

No args = start the REPL. The prompt looks like:

```text
:<account> [<symbol> $<last-price>] >
```

Keys already in the web app do not need re-entering: sign in, start the CLI
and approve the web app once, and accounts sync both ways. See
[Get started → Bring your exchange keys](./getting-started.md#3-bring-your-exchange-keys).

Otherwise, add one:

```bash
tealstreet account add -n mybybit -e bybit -k <KEY> -s <SECRET> --default
```

Then re-launch and it'll auto-focus that account. To see every supported
exchange and the credential fields each one needs:

```bash
tealstreet account exchanges
```

### HyroTrader trial accounts

HyroTrader challenge accounts use Tealstreet's `hyrotrader` exchange adapter,
but the standalone CLI needs the HyroTrader production REST origin in your shell
environment before it can connect. Without it, connection fails with:

```text
HyroTrader production REST URL is not configured
```

Set the production origin, then add the challenge account with the API key and
secret HyroTrader provides for that challenge:

```bash
export NEXT_PUBLIC_HYROTRADER_PROD_REST_URL="https://<hyro-production-origin>"

tealstreet account add -n hyro-trial -e hyrotrader -k <KEY> -s <SECRET> --default
tealstreet interactive --account hyro-trial
```

Use the origin only, not a `/v5/...` path. Public and private WebSocket
URLs are normally derived from that REST origin. See
[Config files → Exchange endpoint overrides](./operations/config-files.md#exchange-endpoint-overrides)
for the optional WebSocket overrides.

### Restricted regions and Bybit 403s

If a direct Bybit connection returns a 403 geo-restriction error, the CLI
needs a different route before exchange requests can succeed. Use a VPN,
or add the account from inside the REPL with Tealstreet proxy routing:

```text
> accounts add -n mybybit -e bybit -k <KEY> -s <SECRET> --tealstreet-proxy --proxy-region SG
```

This only changes how the CLI reaches the exchange. TradingView webhooks
are set up separately — see
[Webhook from TradingView](./recipes/webhook-from-tradingview.md).

---

## Logging in

The CLI uses Auth0 for update checks and identity — separate from any
exchange API keys, which live only on your machine.

From your shell, or as `login` inside the REPL:

```bash
tealstreet login
```

Your browser opens, you sign in, the CLI prints `Logged in as <email>`,
and writes the token to `~/.tealstreet/config.json`. `tealstreet logout` clears it, and
`tealstreet whoami` (alias `auth`) shows the current user.

The OAuth callback listens on **`localhost:9876`** during the flow.

---

## Updating

```bash
tealstreet update       # download and replace the running binary
tealstreet update -c    # version-check only
```

`update --check` (or `-c`) checks the latest release on GitHub without
downloading. The REPL also runs a background check at startup and prints
a banner when a newer version is published.

---

## Top-level flags

| Flag               | Effect                                 |
| ------------------ | -------------------------------------- |
| `-v` / `--version` | Print version and exit                 |
| `--update`         | Trigger updater                        |
| `--check-update`   | Version-check only                     |
| `--disable-all`    | Start REPL with all exchanges disabled |

`--disable-all` is useful when you want to launch the CLI without
auto-connecting every account — for cold-starts on flaky networks, or for
running purely-local commands like `sim on` / `set`.

---

## File locations

The CLI keeps state under `~/.tealstreet/`. All of it is text or SQLite —
inspectable with `cat`, `jq`, or `sqlite3`. See
[Config files](./operations/config-files.md) for the full layout.

| Override                 | Effect                                  |
| ------------------------ | --------------------------------------- |
| `TEALSTREET_CONFIG_DIR`  | Move the entire state directory         |
| `TEALSTREET_CONFIG_FILE` | Override just the main config file path |

---

## Where to next

- **First trade** — [Quickstart](./quickstart.md)
- **Add more accounts** — [Commands → Accounts](./reference/commands.md#accounts)
- **Pair to the web terminal** — [Pairing](./operations/pairing.md)
