# Cypress E2E Testing Guide

This guide covers the Cypress end-to-end testing system for exchange integrations in Tealstreet.

## Table of Contents

- [Quick Start](#quick-start)
- [CLI Usage](#cli-usage)
- [Debugging Test Failures](#debugging-test-failures)
- [Parallel Testing](#parallel-testing)
- [Exchange Configuration](#exchange-configuration)
- [Test Structure](#test-structure)
- [Per-Exchange Test Skipping](#per-exchange-test-skipping)
- [Troubleshooting](#troubleshooting)

## Quick Start

**Always run tests against localhost:3000** (the default). Make sure your dev server is running first.

```bash
# Start the dev server (in a separate terminal)
yarn dev

# Run all tests for a specific exchange (headless)
yarn test:e2e --exchange blofin

# Run with visible browser (recommended for debugging)
yarn test:e2e --exchange blofin --headed

# Run specific test(s)
yarn test:e2e --exchange blofin --tests load-balance,place-market-orders

# Run a single test with visible browser
yarn test:e2e --exchange phemex --tests reduce-orders --headed
```

Only use `--url` when explicitly testing against a deployed environment:

```bash
# Against preview (only when specifically needed)
yarn test:e2e --exchange blofin --url preview --headed
```

## CLI Usage

### Basic Command Structure

All commands must be run from the `apps/web` directory (or use the workspace-level script):

```bash
cd apps/web
yarn test:e2e --exchange <exchange-name> [options]
```

### CLI Options

| Option              | Description                                     | Example                              |
| ------------------- | ----------------------------------------------- | ------------------------------------ |
| `--exchange <name>` | **Required**. Exchange to test                  | `--exchange blofin`                  |
| `--tests <names>`   | Comma-separated list of tests to run            | `--tests load-balance,reduce-orders` |
| `--headed`          | Run with visible browser (useful for debugging) | `--headed`                           |
| `--open`            | Open Cypress UI (interactive mode)              | `--open`                             |
| `--url <target>`    | Override base URL (default: localhost:3000)     | `--url preview`                      |

### URL Aliases

| Alias     | URL                             |
| --------- | ------------------------------- |
| `preview` | `https://preview.tealstreet.io` |
| `v3`      | `https://v3.tealstreet.io`      |
| `dev`     | `https://dev.tealstreet.io`     |

Or pass any full URL: `--url https://custom.example.com`

### Available Test Names

| Test                  | Description                                                            |
| --------------------- | ---------------------------------------------------------------------- |
| `load-balance`        | Account loading and balance checks                                     |
| `place-market-orders` | Market order placement and fill verification                           |
| `place-limit-orders`  | Limit order placement with GTC, Post-Only, cancel+replace editing      |
| `position-mode`       | Position mode switching (One Way / Hedged) with leverage/margin combos |
| `reduce-orders`       | Reduce-only orders and position management                             |
| `hedge-mode`          | Hedged position mode behavior                                          |
| `bracket-orders`      | Bracket orders (TP/SL attached to market orders)                       |
| `margin-slider`       | Leverage and margin adjustments                                        |
| `stop-orders`         | Stop loss and take profit orders across position modes                 |
| `all`                 | Run all available tests                                                |

### Examples

```bash
# Debug a single failing test with visible browser
yarn test:e2e --exchange phemex --tests reduce-orders --headed

# Run multiple tests
yarn test:e2e --exchange woo --tests load-balance,place-market-orders,bracket-orders

# Run all tests for an exchange (headless, for CI)
yarn test:e2e --exchange woo --tests all

# Open interactive Cypress UI
yarn test:e2e --exchange blofin --open
```

## Debugging Test Failures

### 1. API Request/Response Logs

Every test run automatically captures HTTP requests/responses and WebSocket messages to log files in `cypress/logs/`. These are the most useful debugging artifact.

**Log files:**

- `cypress/logs/{exchange}-api.log` — API log from the most recent run
- `cypress/logs/{exchange}-failed-{timestamp}.log` — Logs from failed test runs

**Log format** (reverse chronological — newest first):

```
[2026-02-17T07:29:03.182Z] [HTTPRESPONSE] Response https://...api.phemex.com/g-orders/create?...&reduceOnly=true Success: {"data":{"ordStatus":"Created","execInst":"ReduceOnly",...}}
[2026-02-17T07:29:03.156Z] [HTTPREQUEST] Request /g-orders/create : {"params":{"side":"Buy","ordType":"Limit","reduceOnly":"true",...}}
[2026-02-17T07:29:01.692Z] [PRIVATEWS] {"position_info":{"light":2,"symbol":"ADAUSDT",...}}
```

**Entry types:**

| Type                | Description                                                               |
| ------------------- | ------------------------------------------------------------------------- |
| `[HTTPREQUEST]`     | Outgoing HTTP request (URL, params, headers)                              |
| `[HTTPRESPONSE]`    | HTTP response (status, data, error codes)                                 |
| `[PRIVATEWS]`       | Private WebSocket message (orders, positions, account)                    |
| `[PRIVATEWSSUB]`    | Private WebSocket subscription/ping                                       |
| `[EXCHANGEERROR]`   | Exchange-level error (parsed from API response, e.g. Bitget `code:40001`) |
| `[EXCHANGEWARNING]` | Exchange-level warning (non-fatal issues reported by exchange code)       |

**How to search logs for specific events:**

```bash
# Find all order placements
grep "orders/create\|orders/place\|order.*POST" cypress/logs/phemex-api.log

# Find position updates
grep "position_info\|positions" cypress/logs/phemex-api.log

# Find errors
grep -i "error\|reject\|abort\|code.*400\|code.*500" cypress/logs/phemex-api.log

# Find a specific order by clOrdID
grep "Tealstreet_abc123" cypress/logs/phemex-api.log
```

**Summary line** at the end of each test:

```
[API Log] 317 entries, types: {"httpRequest":127,"httpResponse":126,"privateWs":42,"privateWsSub":22}
```

### 2. Screenshots

Failed tests automatically capture screenshots to `cypress/screenshots/spec.cy.ts/`.

Screenshot naming: `Test {EXCHANGE} Exchange Integration on {SYMBOL} -- {test name} (failed).png`

The screenshot shows the full app UI at the moment of failure, including:

- The order book and chart state
- Open orders and positions tables
- The Cypress command log in the left sidebar (visible in headed mode)

### 3. Cypress Command Log (Headed Mode Only)

When running with `--headed`, the browser's left sidebar shows the step-by-step Cypress command log. This includes:

- Each `cy.get()`, `cy.click()`, assertion
- `cy.log()` messages from the test (e.g., "Placing buy limit at 0.2905")
- Pass/fail status for each assertion

**This is NOT available in headless mode or from the CLI output.** To see it programmatically, use the API log files instead.

### 4. Debugging Workflow

When a test fails:

1. **Check the terminal output** for the error message and line number
2. **Read the screenshot** to see the UI state at failure
3. **Read the API log** to trace the actual requests/responses
4. **Run again with `--headed`** to watch it live if needed

Example debugging session:

```bash
# Test failed — check the error
# Output says: "AssertionError: expected 1 to equal 0" at line 1435

# Read the screenshot
# (Use Read tool on the screenshot path shown in output)

# Read the API log for the last order
grep -A2 "orders/create" cypress/logs/phemex-api.log | head -20

# Run again headed to watch it
yarn test:e2e --exchange phemex --tests reduce-orders --headed
```

### 5. What the API Log Captures

The logging system works by:

1. Auto-enabling the Requests Panel debug tool when Cypress is detected
2. Capturing all `grafana-http-request`, `grafana-http-response`, `grafana-pws`, and `grafana-pws-sub` events from the exchange
3. Flushing captured entries to a log file after each test via `afterEach` hook

This works for **all exchanges** regardless of whether they use HTTP-over-WebSocket (HOW) or standard REST — both paths are captured.

Key browser-side APIs (used by `afterEach` and available in DevTools):

- `window.__getRequestsLog()` — reads all captured log entries
- `window.__clearRequestsLog()` — resets after each test
- `window.__getStoreSnapshot()` — returns current orders and positions from the exchange store

### 6. Failure Diagnostics in Slack

The `testFailureDiagnostics` field in result JSONs (`cypress/results/{exchange}.json`) is sent to Slack and contains:

- Error-severity entries (actual errors from the exchange)
- Recent order/position/algo endpoint requests and responses
- Responses with non-success codes (e.g., Bitget `{"code":"40001"}`, OKX `{"code":"1"}`)
- Store snapshot at failure time (orders and positions from the Jotai store)

## Parallel Testing

For CI/CD pipelines or testing multiple exchanges simultaneously:

```bash
yarn test:e2e:parallel --exchanges <exchange-list> [options]
```

### Parallel CLI Options

| Option                 | Description                                      | Example                                     |
| ---------------------- | ------------------------------------------------ | ------------------------------------------- |
| `--exchanges <list>`   | **Required**. Comma-separated exchanges or `all` | `--exchanges woo,phemex,blofin`             |
| `--tests <names>`      | Tests to run (default: all)                      | `--tests reduce-orders,place-market-orders` |
| `--browser <name>`     | Browser to use (default: chrome)                 | `--browser chrome`                          |
| `--max-parallel <num>` | Max parallel instances (default: 10)             | `--max-parallel 4`                          |
| `--headed`             | Run with visible browser                         | `--headed`                                  |

### SIGTERM Handling (CI)

The parallel runner handles SIGTERM/SIGINT signals gracefully:

- Kills all child Cypress processes
- Parses test results from buffered stdout
- Writes partial result JSONs to `cypress/results/` for the Slack reporter
- Exits with code 1

This ensures that if a CI job times out, results for completed exchanges are still reported.

### Examples

```bash
# Test all exchanges in parallel
yarn test:e2e:parallel --exchanges all

# Test specific exchanges with limited concurrency
yarn test:e2e:parallel --exchanges woo,phemex,blofin --max-parallel 4

# Test all with specific tests only
yarn test:e2e:parallel --exchanges all --tests load-balance,place-market-orders
```

## Exchange Configuration

Exchange settings are defined in `cypress/config/exchanges.js` (single source of truth):

```javascript
blofin: {
  exchange: 'blofin',
  symbol: 'DOGEUSDT',
  orderSizeInQuote: 15,
  orderSizeVariance: 0.2,
  farTouchDelta: 0.015,
  expectedWalletBalance: 100,
  expectedAvailableMargin: 100,
  actionWaitTime: 5000,
  skipTests: ['position-mode'], // set-position-mode returns 400 via proxy
},
```

### Configuration Fields

| Field                     | Type      | Description                                 |
| ------------------------- | --------- | ------------------------------------------- |
| `exchange`                | string    | Exchange identifier                         |
| `symbol`                  | string    | Trading pair to test (e.g., `DOGEUSDT`)     |
| `orderSizeInQuote`        | number    | Order size in quote currency (USD)          |
| `orderSizeVariance`       | number    | Acceptable size variance (0.2 = 20%)        |
| `farTouchDelta`           | number    | Price delta for limit orders (0.015 = 1.5%) |
| `expectedWalletBalance`   | number    | Expected account balance                    |
| `expectedAvailableMargin` | number    | Expected available margin                   |
| `actionWaitTime`          | number?   | Wait time after actions (ms)                |
| `slowUpdateExtraWaitTime` | number?   | Extra wait for slow position updates (ms)   |
| `skipTests`               | string[]? | Tests to skip for this exchange             |

### Currently Active Exchanges

| Exchange    | Symbol    | Notes                                                            |
| ----------- | --------- | ---------------------------------------------------------------- |
| WOO         | ETHUSDT   | REST polling for positions; XRP asks sit above WOO's buy cap     |
| Phemex      | ADAUSDT   | WebSocket positions but can be delayed                           |
| BloFin      | DOGEUSDT  | Best position updates; `set-position-mode` 400s via proxy        |
| Bybit       | DOGEUSDT  | Unified account can't switch margin type (Cross only)            |
| Bitget      | TRXUSDT   | Virtual TPSL via REST polling; REST responses cached ~2s         |
| Bitunix     | LINKUSDT  | Account locked in HEDGE mode                                     |
| Hyperliquid | DOGE-PERP | OneWay mode only; API doesn't report post-only on resting orders |

### API Keys

Exchange API keys are stored in `cypress.env.json` (not committed). Each exchange has a named account with keys configured there.

## Test Structure

### File Organization

```
cypress/
├── e2e/
│   └── spec.cy.ts              # Main test file (all tests)
├── config/
│   └── exchanges.js            # Exchange configurations (single source of truth)
├── support/
│   ├── commands.ts             # Custom Cypress commands
│   ├── debug-logger.ts         # Debug logging utilities
│   └── e2e.ts                  # Global hooks and setup
├── scripts/
│   ├── run-e2e.js              # CLI wrapper (yarn test:e2e)
│   ├── run-e2e-parallel.js     # Parallel runner (yarn test:e2e:parallel)
│   ├── generate-test-summary.js # Test summary generator
│   └── slack-e2e-report.js     # Slack integration for CI reports
├── logs/                       # API logs (gitignored)
├── results/                    # Test result JSONs (gitignored)
├── screenshots/                # Auto-generated on failures
└── README.md                   # This file
```

### Test Lifecycle

1. **before()** — Runs once before all tests
   - Sets cookies and localStorage (accounts, settings, mock user)
   - Intercepts cloud settings API to prevent overriding test state
   - Visits the app and waits for exchange to load (markets, positions, orders, balance)
   - Switches to the test symbol
   - Flattens any existing positions and cancels open orders

2. **afterEach()** — Runs after each test
   - Flushes API log entries from the browser to a log file

3. **Individual tests** — Each `it()` block runs independently

### Key Test Helpers

| Helper                                             | Description                                   |
| -------------------------------------------------- | --------------------------------------------- |
| `setOrderSize(quoteSize)`                          | Set order size in quote currency              |
| `setBuyPrice(price)` / `setSellPrice(price)`       | Set limit order prices                        |
| `ensureTableBodyLength(selector, count, timeout?)` | Assert table row count                        |
| `ensurePositionSize(quoteSize, side, symbol)`      | Assert position exists with correct size/side |
| `flattenPositions()`                               | Close all positions with retry logic          |
| `setSelectValue(selector, value)`                  | Set a custom Select component value           |
| `ensureSelectValue(selector, value)`               | Assert a Select component's current value     |
| `ensureCheckboxIsChecked(selector, eq)`            | Ensure checkbox is checked (click if not)     |
| `getLastTrade()`                                   | Fetch current market price from ticker        |
| `handleRoundingError(price)`                       | Round price to exchange tick size             |

### Position Wait Timeouts

Exchanges with slow position updates use `positionWaitTimeout` (derived from `slowUpdateExtraWaitTime`):

- **Default**: 45s (exchanges with fast WS position updates)
- **Slow exchanges** (WOO, Phemex): 90s (relies on REST polling)

Always pass `positionWaitTimeout` when asserting position appearance/disappearance:

```typescript
ensureTableBodyLength('[data-testid=open-positions-table]', 1, positionWaitTimeout);
ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
```

## Per-Exchange Test Skipping

### Clearing leftover dust (`allow_foreign_flatten`)

`assertNoForeignPositions()` refuses to run when an account holds a position
outside its configured test symbol, because `closeAllPositionsViaController()`
is symbol-agnostic and would trade real size. To clear leftover dust
deliberately, dispatch the workflow with the **`allow_foreign_flatten`** input
checked — the guard then logs each foreign position and lets the flatten
proceed:

```bash
gh workflow run e2e.yaml --ref <branch> \
  -f target=preview -f exchanges=okx -f tests=load-balance \
  -f allow_foreign_flatten=true
```

Do this through the workflow, never with hand-rolled signed REST — that
bypasses the proxy/tunnel routing IP-whitelisted keys require and returns
`50110 invalid IP`. The `schedule` trigger cannot set the input, so the nightly
always keeps the hard guard. Re-run with the flag off afterwards to confirm the
account is clean.

### Current Skip Configuration

**Every `skipTests` array is empty.** The accumulated list was masking real
defects rather than recording venue limitations — bybit's two skips hid a stale
`fetchPositionMode`, blofin's hid a 400 from `set-position-mode` — so the whole
set was cleared in favour of real per-exchange results.

Before adding an entry back, prove the test _cannot_ pass. A venue capability
that is checkable in `exchangeMetadata` (e.g. `positionModes: [OneWay]`) belongs
as a metadata-driven gate in the spec, not a per-exchange skip list. Put the
evidence in the comment.

### How Skipping Works

In `exchanges.js`:

```javascript
blofin: {
  skipTests: ['position-mode'],
}
```

The `shouldRunTest()` function checks both the CLI `--tests` filter and the exchange's `skipTests` array. The CLI runner also warns when requested tests are in skipTests.

### Exchange-Specific Test Adaptations

Some tests adapt behavior per-exchange without fully skipping:

- **BloFin stop-orders**: Runs only in Hedged mode (can't switch position modes)
- **Phemex stop-orders**: Skips Hedged mode (TE_REDUCE_ONLY_ABORT when switching with pending TP/SL)
- **Phemex reduce-orders**: Uses `positionWaitTimeout` for position close verification (slow WS updates)
- **Hyperliquid place-limit-orders**: Skips TIF column assertion (API doesn't report post-only on resting orders, always shows GTC)
- **Hyperliquid position-mode**: Only tests margin type switching (Cross/Isolated), not position mode switching (OneWay only)

## Troubleshooting

### Test Fails with Timeout on Position Check

**Symptom**: `expected 1 to equal 0` or `expected 0 to equal 1` on `open-positions-table`

**Cause**: Exchange has slow position updates via WebSocket. The default `ensureTableBodyLength` timeout (30s) isn't enough.

**Fix**: Pass `positionWaitTimeout` to the assertion:

```typescript
ensureTableBodyLength('[data-testid=open-positions-table]', 0, positionWaitTimeout);
```

### Test Fails with Position Mode Switch Error

**Symptom**: `expected 'Hedged' to equal 'One Way'` on `position-mode-selector`

**Cause**: Exchange proxy returns 400 for `set-position-mode` API call.

**Fix**: Either skip the test (`skipTests: ['position-mode']`) or filter `positionModes` array to only include the mode the exchange is stuck in.

### TE_REDUCE_ONLY_ABORT Uncaught Exception

**Symptom**: Cypress catches `Error: TE_REDUCE_ONLY_ABORT` from the app

**Cause**: Phemex rejects reduce-only operations when there's no position or position mode conflicts.

**Fix**: The `e2e.ts` support file has an `uncaught:exception` handler that suppresses known exchange errors. If a new error type appears, add it there.

### Exchange Fails to Load

**Symptom**: `toggle-tabs-limit` never appears, test times out during setup

**Cause**: Account has insufficient balance, or exchange API is down.

**Fix**: Check account balance on the exchange. The `load-balance` test verifies this.

### VPN Required

Some exchanges (e.g., BloFin) require a VPN connection (German VPN for BloFin proxy). If tests fail with connection errors, ensure VPN is active.

### Authentication

Tests authenticate via real Auth0 login using `testUsername` and `testPassword` from `cypress.env.json`. If auth fails:

1. Check that `cypress.env.json` has valid `testUsername` and `testPassword`
2. Verify the Auth0 test account is still active
3. Check that the Auth0 login form selectors haven't changed

## CI/CD Integration

### GitHub Actions

The E2E workflow (`.github/workflows/e2e.yaml`) runs on a self-hosted runner and can be triggered manually via the "Run workflow" button on GitHub, or automatically on a daily schedule.

The workflow dropdown provides:

- **Target environment**: preview, v3, or dev
- **Exchanges to test**: individual exchange or "all"
- **Tests to run**: individual test or "all"

When "all" exchanges is selected, the parallel runner (`run-e2e-parallel.js`) reads `Object.keys()` from `cypress/config/exchanges.js` and runs every configured exchange concurrently.

### Adding a New Exchange

To add a new exchange to the E2E test suite:

1. **Add exchange config** in `cypress/config/exchanges.js`:

   ```javascript
   myexchange: {
     exchange: 'myexchange',
     symbol: 'DOGEUSDT',
     orderSizeInQuote: 10,
     orderSizeVariance: 0.2,
     farTouchDelta: 0.015,
     expectedWalletBalance: 100,
     expectedAvailableMargin: 100,
     skipTests: [],  // add tests to skip if needed
   },
   ```

2. **Add API keys** in `cypress.env.json` (local) with a named account matching the exchange, AND update the `CYPRESS_ENV_JSON` GitHub secret

3. **Add to GitHub Actions dropdown** in `.github/workflows/e2e.yaml` under `inputs.exchanges.options`:

   ```yaml
   options:
     - all
     - woo
     - myexchange # <-- add here
   ```

   The "all" option automatically picks up the exchange from `exchanges.js`, but adding it to the dropdown allows running it individually from the UI.

4. **Handle exchange-specific quirks** — if the exchange has API limitations:
   - Set `skipTests` in `exchanges.js` for tests that can't work
   - Override `positionModes` / `hidePositionModeSelector` in `exchangeMetadata.ts` if position mode support differs from defaults
   - Add conditional logic in `spec.cy.ts` for assertion differences (e.g., `if (getAccMetadata().exchangeName !== 'myexchange')`)

### Artifacts

On failure, these artifacts are uploaded:

- `e2e-screenshots` — Screenshots from failed tests
- `e2e-api-logs` — API request/response logs for debugging
