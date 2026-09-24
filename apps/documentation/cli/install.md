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
# One-liner — detects platform/arch, drops into /usr/local/bin
curl -fsSL https://get.tealstreet.io/cli | bash

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

**There is no native Windows build yet** — every release ships the four
assets in the table below and nothing else. Install inside WSL with the
macOS/Linux one-liner above; the `linux-x64` binary runs there.

## Available builds

| Asset                     | Platform           |
| ------------------------- | ------------------ |
| `tealstreet-darwin-arm64` | Apple Silicon Macs |
| `tealstreet-darwin-x64`   | Intel Macs         |
| `tealstreet-linux-x64`    | Linux x86_64       |
| `tealstreet-linux-arm64`  | Linux arm64        |

---

## First run

```bash
tealstreet
```

No args = start the REPL. The prompt looks like:

```text
:<account> [<symbol> $<last-price>] >
```

If you have no accounts yet, add one:

```bash
tealstreet account add -n mybybit -e bybit -k <KEY> -s <SECRET> --default
```

Then re-launch and it'll auto-focus that account. To see every supported
exchange and the credential fields each one needs:

```bash
tealstreet account exchanges
```

### Restricted regions and Bybit 403s

If a direct Bybit connection returns a 403 geo-restriction error, the CLI
needs a different route before exchange requests can succeed. Use a VPN,
or add the account from inside the REPL with Tealstreet proxy routing:

```text
> accounts add -n mybybit -e bybit -k <KEY> -s <SECRET> --tealstreet-proxy --proxy-region SG
```

This only changes how the CLI reaches the exchange. It does not make
TradingView webhooks reach the CLI; the hosted TradingView-to-CLI relay is
not live yet.

---

## Logging in

The CLI uses Auth0 for update checks and identity — separate from any
exchange API keys, which live only on your machine.

Inside the REPL:

```text
> login
```

Your browser opens, you sign in, the CLI prints `Logged in as <email>`,
and writes the token to `~/.tealstreet/config.json`. `logout` clears it.
`whoami` (alias `auth`) shows the current user.

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
