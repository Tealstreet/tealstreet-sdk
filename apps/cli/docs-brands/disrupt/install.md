# Install

Standalone binaries are published to
[`DisruptTrade/cli`](https://github.com/DisruptTrade/cli/releases). There is
no package-manager channel: download the binary for your platform, make it
executable and put it on your `PATH`.

## macOS & Linux

```bash
# Apple Silicon — swap the asset name for your platform (see the table below)
curl -fsSL https://github.com/DisruptTrade/cli/releases/latest/download/disrupt-darwin-arm64 -o /usr/local/bin/disrupt
chmod +x /usr/local/bin/disrupt
```

Verify:

```bash
disrupt --version
```

If you see `command not found`, `/usr/local/bin` isn't on your `PATH` —
add it, or symlink the binary somewhere that is.

Every release ships a `SHA256SUMS` file alongside the binaries if you want
to verify the download.

## Windows

Download `disrupt-windows-x64.exe` from the releases page and put it
somewhere on your PATH.

Two things to know:

- **Run it from Windows Terminal or PowerShell 7.** The REPL draws a pinned
  top bar and bottom footer using ANSI escape sequences. Legacy `cmd.exe`
  does not render those correctly.
- **`disrupt update` is not supported on Windows yet.** Download the new
  `.exe` again to upgrade.

WSL remains a fine alternative — the `linux-x64` binary runs there with the
macOS/Linux steps above.

## Available builds

| Asset                     | Platform           |
| ------------------------- | ------------------ |
| `disrupt-darwin-arm64`    | Apple Silicon Macs |
| `disrupt-darwin-x64`      | Intel Macs         |
| `disrupt-linux-x64`       | Linux x86_64       |
| `disrupt-linux-arm64`     | Linux arm64        |
| `disrupt-windows-x64.exe` | Windows x64        |

---

## First run

```bash
disrupt
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
disrupt account add -n mybybit -e bybit -k <KEY> -s <SECRET> --default
```

Then re-launch and it'll auto-focus that account. To see every supported
exchange and the credential fields each one needs:

```bash
disrupt account exchanges
```

### HyroTrader trial accounts

HyroTrader challenge accounts use the `hyrotrader` exchange adapter, but the
standalone CLI needs the HyroTrader production REST origin in your shell
environment before it can connect. Without it, connection fails with:

```text
HyroTrader production REST URL is not configured
```

Set the production origin, then add the challenge account with the API key and
secret HyroTrader provides for that challenge:

```bash
export NEXT_PUBLIC_HYROTRADER_PROD_REST_URL="https://<hyro-production-origin>"

disrupt account add -n hyro-trial -e hyrotrader -k <KEY> -s <SECRET> --default
disrupt interactive --account hyro-trial
```

Use the origin only, not a `/v5/...` path. Public and private WebSocket
URLs are normally derived from that REST origin. See
[Config files → Exchange endpoint overrides](./operations/config-files.md#exchange-endpoint-overrides)
for the optional WebSocket overrides.

### Restricted regions and Bybit 403s

If a direct Bybit connection returns a 403 geo-restriction error, the CLI
needs a different route before exchange requests can succeed. Use a VPN,
or add the account from inside the REPL with proxy routing:

```text
> accounts add -n mybybit -e bybit -k <KEY> -s <SECRET> --proxy --proxy-region SG
```

This only changes how the CLI reaches the exchange. TradingView webhooks
are set up separately — see
[Webhook from TradingView](./recipes/webhook-from-tradingview.md).

---

## Logging in

The CLI uses your web-app account for update checks and identity — separate
from any exchange API keys, which live only on your machine.

From your shell, or as `login` inside the REPL:

```bash
disrupt login
```

Your browser opens, you sign in, the CLI prints `Logged in as <email>`,
and writes the token to `~/.disrupt/config.json`. `disrupt logout` clears it, and
`disrupt whoami` (alias `auth`) shows the current user.

The OAuth callback listens on **`localhost:9876`** during the flow.

---

## Updating

```bash
disrupt update       # download and replace the running binary
disrupt update -c    # version-check only
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

The CLI keeps state under `~/.disrupt/`. All of it is text or SQLite —
inspectable with `cat`, `jq`, or `sqlite3`. See
[Config files](./operations/config-files.md) for the full layout.

| Override              | Effect                                  |
| --------------------- | --------------------------------------- |
| `DISRUPT_CONFIG_DIR`  | Move the entire state directory         |
| `DISRUPT_CONFIG_FILE` | Override just the main config file path |

---

## Where to next

- **First trade** — [Quickstart](./quickstart.md)
- **Add more accounts** — [Commands → Accounts](./reference/commands.md#accounts)
- **Pair to the web terminal** — [Pairing](./operations/pairing.md)
