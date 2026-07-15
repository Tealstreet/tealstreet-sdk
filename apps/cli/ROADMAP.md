# Tealstreet CLI - Roadmap

## Current Status (v0.1.0) ✅

**Core Infrastructure:**

- [x] Build & release system (bun compile for macOS/Linux)
- [x] Auto-update checker with GitHub Releases
- [x] Config management (`~/.tealstreet/`)
- [x] Account management (add, remove, default)
- [x] Interactive REPL mode
- [x] Connection monitoring

**Trading Commands (via safe-cex):**

- [x] `buy` / `sell` - Market and limit orders
- [x] `close` - Close position (% or all)
- [x] `cancel` - Cancel orders (all, buys, sells, stops)
- [x] `scale` - Scale orders (ladder)
- [x] `chase` - Chase orders
- [x] `stop` - Stop orders
- [x] `nuke` - Cancel all & close all
- [x] `balance` - Show balance
- [x] `positions` / `position` - View positions
- [x] `wa` / `watch account` - Real-time position/order/margin monitoring
- [x] `chart` - ASCII candlestick charts

**Exchanges Supported:**

- [x] Bybit, Binance, Hyperliquid, BloFin, Bitget, WOO, OKX, BitMEX, Coinbase, Kraken, KuCoin, Phemex, Paradex

---

## Phase 1: Core UX Improvements 🎯

### 1.1 Aliases/Hotkeys (ichibot parity)

- [ ] `alias` command - Create custom command shortcuts
- [ ] Persistent aliases in `~/.tealstreet/aliases.json`
- [ ] Single-key execution (e.g., `[` → `buy 0.001`)
- [ ] Built-in sensible defaults

### 1.2 Fatfinger Protection

- [ ] `fatfinger` command - Set max order size
- [ ] Per-symbol fatfinger limits
- [ ] Confirmation prompts for large orders
- [ ] "Are you sure?" for position-closing orders

### 1.3 Quick Symbol Switching

- [ ] `@` prefix for quick symbol (e.g., `@ETH buy $100`)
- [ ] Symbol favorites list
- [ ] Recent symbols history
- [ ] Tab completion for symbols

### 1.4 Command History & Persistence

- [x] Arrow key history navigation (readline default)
- [x] Persistent history in `~/.tealstreet/cli_history`
- [x] `history` command to view/search (shipped `e9ce493e74`)
- [ ] Ctrl+R reverse search

---

## Phase 2: Advanced Order Types 🚀

### 2.1 TWAP (Time-Weighted Average Price)

- [ ] `twap buy $1000 over 1h` - Split order over time
- [ ] Configurable intervals
- [ ] Jitter option (randomize timing for stealth)
- [ ] Size randomization option
- [ ] Progress tracking in background
- [ ] `twaps` command to view active TWAPs
- [ ] Cancel running TWAP

### 2.2 Enhanced Scale Orders

- [ ] Cubic/Exponential distribution (not just linear)
- [ ] Price distribution slider equivalent
- [ ] `scale buy $1000 5 orders from +0.5% to +2% exponential`
- [ ] Preview before execution

### 2.3 Bracket Orders

- [ ] `bracket buy $100 tp 5% sl 2%` - Entry with TP/SL
- [ ] OCO (One-Cancels-Other) support
- [ ] Trailing stop support

### 2.4 DCA (Dollar Cost Averaging)

- [ ] `dca buy $100 every 1h for 24h`
- [ ] Recurring scheduled orders
- [ ] Background execution

---

## Phase 3: Multi-Account & Portfolio 📊

### 3.1 Multi-Account Support

- [ ] `switch <account>` - Quick account switching
- [ ] `accounts` - List all with connection status
- [ ] Parallel commands across accounts
- [ ] `@all buy $100` - Execute on all accounts

### 3.2 Portfolio Overview

- [ ] `portfolio` - Aggregate view across exchanges
- [ ] Total equity, PnL, exposure
- [ ] Position breakdown by asset
- [ ] Risk metrics

### 3.3 Account Sync

- [ ] Copy trades between accounts
- [ ] Position mirroring
- [ ] Size scaling per account

---

## Phase 4: Automation & Scripting 🤖

### 4.1 Command Scripts

- [ ] Load commands from file: `tealstreet run script.ts`
- [ ] Conditional execution
- [ ] Variables and expressions
- [ ] Loop constructs

### 4.2 Price Alerts

- [ ] `alert BTC > 100000 then buy $1000`
- [ ] Background alert monitoring
- [ ] Webhook notifications
- [ ] Telegram integration

### 4.3 Event Triggers

- [ ] On fill → execute command
- [ ] On price cross → execute command
- [ ] On position change → notify

---

## Phase 5: TUI Mode 🖥️

### 5.1 Full Terminal UI

- [ ] Split panes (positions, orders, chart, command)
- [ ] Real-time updates
- [ ] Keyboard navigation
- [ ] Mouse support (optional)

### 5.2 Live Charts

- [ ] Real-time candlestick updates
- [ ] Multiple timeframes
- [ ] Order/position overlays
- [ ] Volume bars

### 5.3 Order Book Visualization

- [ ] Depth chart in terminal
- [ ] Bid/ask spread display
- [ ] Large order highlighting

---

## Phase 6: Distribution 📦

### 6.1 Package Managers

- [ ] Homebrew tap (`brew install tealstreet/tap/tealstreet`)
- [ ] apt repository for Linux
- [ ] npm/npx support (`npx @tealstreet/cli`)

### 6.2 Auto-Update

- [x] Version check on startup
- [x] Notification for new versions
- [ ] Self-update command (`tealstreet update`)
- [ ] Auto-update option (with user consent)

### 6.3 Windows Support

- [ ] Windows binary builds
- [ ] Chocolatey package
- [ ] Scoop manifest

---

## Backlog / Ideas 💡

- Stream Deck integration
- Voice commands
- AI assistant mode ("buy some Bitcoin")
- Backtesting mode
- Paper trading mode
- Custom indicators for alerts
- Webhook server for external triggers
- Integration with trading journals
- PnL tracking and analytics
- Tax report generation

---

## Priority Matrix

| Feature              | Impact | Effort | Priority |
| -------------------- | ------ | ------ | -------- |
| Aliases/Hotkeys      | High   | Low    | P0       |
| Fatfinger Protection | High   | Low    | P0       |
| Command History      | Medium | Low    | P0       |
| TWAP                 | High   | Medium | P1       |
| Enhanced Scale       | Medium | Low    | P1       |
| Bracket Orders       | High   | Medium | P1       |
| Multi-Account        | High   | Medium | P2       |
| TUI Mode             | High   | High   | P2       |
| Homebrew             | Medium | Low    | P2       |
| Scripts              | Medium | High   | P3       |
| Alerts               | High   | High   | P3       |

---

## Sources

- [Ichibot Wiki](https://gitlab.com/Ichimikichiki/ichibot-client-app/-/wikis/home)
- [In Silico Terminal Docs](https://docs.insilicoterminal.com/documentation/terminal-cli/scale-orders)
