---
title: Get started
sidebar_position: 1
---

# Get started

Install, sign in, bring your exchange keys, link the web app, trade safely,
hand it to an AI agent if you like, and stay current.

## 1. Install

```bash
# macOS / Linux one-liner (asks for sudo if /usr/local/bin is not writable)
curl -fsSL https://raw.githubusercontent.com/Tealstreet/cli/main/install.sh | bash

# or Homebrew
brew install tealstreet/tap/tealstreet

# or npm (any platform, including Windows)
npm install -g @tealstreet/cli
```

Check it worked:

```bash
tealstreet --version
```

More options, including direct downloads: [Install](./install.md).

## 2. Sign in

```bash
tealstreet login
```

Your browser opens; sign in with your Tealstreet account and the CLI prints
`Successfully logged in as <email>`. Sign-in covers identity, update checks and
proxy routing. Your API secret stays on this machine and requests are signed
here; an account set to use Tealstreet's proxy (below) sends the signed request,
including its API key header, through that proxy.

```bash
tealstreet whoami
```

shows where you stand. When it reports `Token expires in: 0 days` or
`Token EXPIRED`, run `tealstreet login` again.

## 3. Bring your exchange keys

### Already set up in the web app? Sync them

You do not need to type your keys again. When the CLI and the web app run on
the same computer, they share accounts both ways:

1. Sign in to the CLI with the **same Tealstreet account** you use in the web
   app (step 2). That is what lets the web app find your CLI.
2. Start the CLI: `tealstreet`.
3. Open (or reload) Tealstreet in your browser or the desktop app on that
   computer, after the CLI is running.
4. The web app asks whether to connect to the CLI. Choose **Yes** (choosing
   **No** declines for the rest of that browser session).
5. The CLI then asks `Approve? [y/N]`. Type `y` within **30 seconds**. If you
   miss it, the web app tries again a minute later.

From then on, every account in the web app appears in the CLI, and accounts you
add in the CLI appear in the web app. This copies your API keys and secrets
between the two over a local-only connection (`127.0.0.1`); they do not pass
through Tealstreet's servers. Approve only browsers you trust. At the `tealstreet`
prompt, `clients` lists approved browsers and `clients revoke <id>` removes one.

Once paired, that browser keeps connecting automatically. Details:
[Listener](./operations/listener.md).

### Or add a key directly

```bash
tealstreet account add -n mybybit -e bybit -k <KEY> -s <SECRET> --default
tealstreet account exchanges   # every exchange and the fields it needs
```

If Bybit answers with a 403 geo-block, or your key is IP-whitelisted, route it
through Tealstreet's proxy (this needs step 2):

```bash
tealstreet account add -n mybybit -e bybit -k <KEY> -s <SECRET> --tealstreet-proxy --proxy-region SG
```

## 4. Link the web app's CLI panel

The CLI panel inside the web app can send its commands to this CLI instead of
running them in the browser. With the two linked (step 3), new CLI panels
already target **`listener`**; an existing panel switches with the `listener`
tab in its tab bar. See
[Listener from web](./recipes/listener-from-web.md).

## 5. Your first order, safely

```text
tealstreet                 # opens the prompt on your default account
> switch BTCUSDT           # focus a symbol
> status                   # confirm account and symbol
> sim on                   # orders are simulated, nothing reaches the exchange
> buy $100 stop $last -1%  # $100 market buy, stop 1% below the current price
> ps                       # positions
> o                        # open orders
> close                    # close the position
> sim off                  # back to real orders when you are ready
```

Every command also runs one-shot from your shell through `exec` (`--json`
output needs CLI 0.10.18 or newer; `tealstreet update` gets it). Keep the
command in **single quotes** so your shell leaves `$100` alone:

```bash
tealstreet exec -a mybybit -s BTCUSDT 'buy $100 stop $last -1%'
```

More recipes: [Quickstart](./quickstart.md).

## 6. Let an AI agent drive it (optional)

An assistant that can run shell commands (Claude Code, Cursor, OpenCode) can
trade through this CLI for you. Connect it to the
[Tealstreet MCP connector](/docs/trade/modules/mcp-connector); its
`get_cli_reference` tool teaches the agent to check the CLI is installed, walk
you through anything missing, and run commands with
`tealstreet exec --json -a <account> -s <symbol> '<command>'`, confirming each
order with you first. Results and codes:
[Machine-readable output](./reference/commands.md#json).

## 7. Stay current

```bash
tealstreet update
```
