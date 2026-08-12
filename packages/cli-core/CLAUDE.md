# CLI Core Package

Shared utilities for the Tealstreet CLI and web terminal.

## Account commands

- `accounts add` and `accounts import` must normalize new accounts through
  `@tealstreet/safe-cex/src/accounts/normalizeExchangeAccountConfig` before
  persisting. Manual add supports `--tealstreet-proxy` / `--proxy-region` so
  standalone CLI can express the same saved account routing shape as the web
  "Use Tealstreet IPs" checkbox.

## Strategy commands

- **Swarm** (`src/commands/trading/SwarmCommand.ts`) — fires N
  concurrent market orders summing to a target size. `irregular`
  randomises slice weights in [0.5x..1.5x of mean] then renormalises
  so the sum still lands on the requested total. Uses
  `Promise.allSettled` so a single bad slice doesn't tank the rest.
  If any slice rounds to zero at the market's amount precision, the
  command errors before placing — better than firing N-k orders
  silently.
- **TWAP chase** (`src/commands/trading/TwapCommand.ts` →
  `handleChaseBuySell`) — `twap chase` parses
  `timeout <pct%> <market|next>` and `fail <market|next>`, strips
  them from argv, delegates the rest to `handleBuySell`, then patches the resulting
  `taskAction.params` with the chase policy. The runtime enforcement
  lives in `safe-cex/src/webworker/twap/TwapInstance.ts` →
  `scheduleSliceTimeout`: at `timeoutPct%` of the slice interval the
  chaser is settled by state — active gets `timeoutAction`,
  stopped-with-remaining gets `failAction`, stopped-and-filled is a
  no-op. `terminate()` cancels pending slice timers.

## Preprocess pipeline (`src/preprocess/`)

Every command string runs through `preprocessCommand` before dispatch:

1. **`expandRepeats`** — `repeat N (...)` blocks expand recursively
   into `N` `;`-separated copies. Caps: 1000 iterations, 8 levels of
   nesting.
2. **`splitChain`** — splits at top-level `;` and `&`, paren- and
   quote-aware so `repeat 5 (a; b)` and `alias x "buy 1; sell 1"`
   stay together.
3. **`tryExpandLoop`** (per-sub-segment, after alias expansion) —
   `loop <variant>: <body>` fans out into one step per matching
   position. Variants: `longs`, `shorts`, `openpos`/`open`,
   `allpos`/`all`. Symbols are appended to the body (`<body> <SYM>`)
   so the trailing-instrument convention picks them up. Aliasable.
   See `src/preprocess/loop.ts`.
   3.5 **`tryExpandIfElse`** (per-sub-segment, before loop expansion) —
   `if (<cond>) (<then>) [else (<else>)]` evaluates the condition
   once against current store state and recursively preprocesses
   the chosen branch's body. Outer chain modifiers (`retry`,
   `errorMode`) propagate to inner steps that didn't set their own.
   Condition vocab shared with `WaitCommand` via
   `src/preprocess/conditions.ts` (`parseCondition`). Throws on
   malformed if-forms; bare `if` with no parens after passes
   through as a literal command.
4. **Per-segment chain modifiers** — `set ec` (no value), `set ro`,
   `set po`, plus `unset ro` / `unset po` are detected and treated
   as chain-level flags for subsequent steps; the marker steps
   themselves aren't emitted. `set ec` makes subsequent steps'
   errors print as warnings (`errorMode: 'continue'`). `set ro` /
   `set po` inject the `reduce` / `po` keywords into subsequent
   order-placing commands via `applyChainOrderModifiers` (idempotent;
   only fires on heads in `RO_INJECT_HEADS` / `PO_INJECT_HEADS`).
   `retry N <body>` peels the prefix and attaches `retry: N` to the
   resulting step (cap N=10; out-of-range falls through to a normal
   step). `@<name> = <body>` (capture syntax) peels the prefix and
   attaches `captureTo: <name>` to every step the body produces;
   the runner awaits captured steps and writes
   `result.data.orderIds.join(',')` into the named user var via
   `setVar`. Detected BEFORE the legacy `@cmd` parallel-prefix
   strip so the capture's `@<name>` isn't lost. A bare `end`
   segment (matched AFTER alias expansion, so `alias bail end`
   works) flips a chain-level `endEarly` flag and stops emitting;
   the flag also propagates upward through the `if`-recursion so
   `if (cond) (end)` cuts the outer chain, not just the branch
   body. `PreprocessResult.endEarly` surfaces the bit for tests /
   introspection. All chain modifiers happen in `preprocess.ts`
   before alias / loop / substitution so they compose naturally
   with `repeat 5 (retry 2 buy 1$)` etc.

   **Forward-reference captures (single-chain).** preprocessCommand
   walks segments in order with a `pendingCaptures: Set<string>`.
   When a later segment's substitute layer sees `@<name>` whose
   `<name>` matches an earlier captureTo target, the token is left
   LITERAL in the emitted step and recorded in
   `step.deferredSubstitution` (sorted string[]). The runner reads
   each name from the store and string-replaces every `@<name>` in
   the command before dispatching — by then the prior capture's
   writeback has fired. This is what makes `@o1 = buy …; cancel @o1`
   work in one chain. Math operands referencing pending captures
   throw at preprocess with a clear "capture vars hold a comma-list,
   not a number" message (capture targets aren't numeric).

5. **`preprocessSegment`** → **`tryEvalMathToken`** (per token) —
   within-token arithmetic on numeric operands (literals,
   `$constants`, `@vars`). Operators `+ - * /` with standard
   precedence, left-to-right within a level. Token must have at
   least one operator between two operand-shaped chunks; tokens like
   `$entry`, `100$`, `5%`, `BTCUSDT/USDT` fall through unchanged.
   See `src/preprocess/math.ts`. The two-token `$entry +N%` shape
   still handled by the dedicated branch in `substituteTokens`
   above the math fallback.
6. **`preprocessSegment`** (per-segment) — expands a leading alias
   (recursive, capped at 8 levels), substitutes `$constants`
   (`$size`/`$lsize`/`$ssize` / `$entry`/`$lentry`/`$sentry` /
   `$bid`/`$ask`/`$last`/`$mark`/`$index` — all eight price anchors
   share the same inline `+N`/`-N`/`+N%`/`-N%` offset combining
   path) and `@vars` against the supplied
   `PreprocessStore`. UI-only constants (`$click`, `$ordersize`,
   `$quick<1-7>`) resolve through `PreprocessContext.uiResolvers`
   when present (the web CLI provides them via
   `apps/web/src/components/cli/uiTokenResolvers.ts`); when absent
   (standalone CLI), the substitute layer throws "only available in
   the web terminal".
   `@<name>` resolution checks **tracker → live → static**. Trackers
   (`track max/min @x SOURCE`) re-evaluate the source via
   `resolveTrackedVar` (in `trackedVars.ts`) AND update the stored
   extreme as a side-effect — `echo @hi` ratchets, `print @hi`
   doesn't (PrintCommand uses the read-only path). Live vars
   (`sets @x SOURCE`) re-evaluate their source via
   `evaluateLiveVarSource` (in `reactiveVars.ts`) on every
   substitute call. Cycles in the source graph
   (`sets @a @b; sets @b @a`, or trackers referencing themselves
   transitively) are caught via the `seen: Set<string>` threaded
   through `substituteTokens`, `buildSubstituteOperandResolver`,
   AND `resolveTrackedVar`. **Three-way storage mutex** (setVar /
   setLiveVar / setTrackedVar each clear the other two) guarantees
   at most one binding per name. **Inside `wait` segments**,
   `substituteTokens` is called with `keepLiveVarsLiteral: true`,
   which defers **both** live AND tracker tokens — the wait poller
   calls `parseCondition` which handles `@<name>` as a numeric LHS,
   resolves through the same precedence per tick, and the tracker
   ratchets per poll. Static @-vars STILL substitute inside wait
   args (they're frozen by design, so there's no value in
   deferring).
7. **Step classification** — `sleep ...` segments come back with
   `block: true`. **Chain separators now carry semantics:** a step
   preceded by `;` gets `awaitPrior: true` (set on the first step a
   `;`-segment emits; alias multi-statement bodies honor their own
   internal `;`/`&`; loop/if fan-outs stay parallel among themselves).
   The runner tracks in-flight fire-and-forget steps and **drains them
   before any `awaitPrior` step** (a barrier) — that's what makes
   `close all; buy $1000` wait for the close. `&` steps don't drain.
   `block` steps (`sleep`/`wait`), `retry`, and `captureTo` steps are
   awaited inline regardless. `expandRepeats` joins iterations with
   `;`, so `repeat 5 (buy 1; sleep 1m)` paces sequentially. (This
   replaced the original Phase-0 "both `;`/`&` fire-and-forget" rule —
   TSBUG-106.)

### Runner failure contract

`executeCommand` preserves `CommandResult.success` / `error` on its
`ExecuteResult`. Runners must treat `handled && success === false` as
a failed step even though no exception was thrown. This is load-bearing
for `retry`, `set ec`, `wait` timeouts, and capture writeback: command
implementations usually return `this.error(...)` instead of throwing,
so dropping this bit makes failed commands look successful.

### Special-cased commands

- `set @<name> VALUE` — preserves `@<name>` literally, substitutes
  VALUE. So `set @x $entry` snapshots the current entry price.
- `unset` / `print` / `alias` / `unalias` — full passthrough, no
  substitution. The `alias` body intentionally stays dynamic so
  `alias x "buy at $entry"` re-resolves `$entry` on every use.

### `PreprocessStore` implementations

- **`MemoryPreprocessStore`** (in this package) — used by tests and
  by the web CLI.
- **`FilePreprocessStore`** (`apps/cli/src/preprocess-store.ts`) —
  persists to `~/.tealstreet/vars.json` + `aliases.json`.

If you add a new substitution token or a new special-cased command,
add tests in `src/preprocess/*.test.ts` and update the user-facing
reference at `apps/documentation/cli/reference/commands.md` in the same commit.

## Test conventions

- **Per-command unit tests** live next to each command (e.g.
  `BuyCommand.test.ts`, `StopCommand.test.ts`,
  `ScaleCommand.test.ts`) with one mock exchange + one mock adapter
  helper per file. The mock pattern is consistent: `placeOrder` /
  `placeOrders` / `cancelOrders` are vitest spies, `markets` and
  `tickers` carry their `*BySymbol` indices so the chaser/twap
  paths don't trip on undefined hashmaps.
- **Cross-feature integration tests** live in
  `src/preprocess/preprocessIntegration.test.ts` — these exercise
  `preprocessCommand` end-to-end against the full layered pipeline
  (`repeat × aliases × vars × $constants × sleep × UI resolvers`).
- **`QUIRK:` describe blocks** mark tests that lock in _intentional_
  limitations of the current architecture, not bugs. They live in a
  dedicated `describe('QUIRK: …')` group. Each test name is prefixed
  `QUIRK:`, with a comment explaining the workaround for users and
  the unbreak-me recipe for code. **Don't delete a QUIRK test
  silently** — if you intend to flip the behavior, update the test
  to assert the new shape and update `apps/documentation/cli/` in the same commit
  so user-facing docs match.

  Current locked-in quirks: none. The two long-standing quirks
  (`$entry +1%` not combining, multi-statement alias bodies not
  re-splitting) were both flipped on 2026-05-10. The corresponding
  integration tests now lock in the _new_ shape — see the
  "$entry inline percent offset" and "multi-statement alias
  bodies splice back into the chain" describe blocks in
  `preprocessIntegration.test.ts`.

## Order sizing: "linear" does NOT imply 1 contract = 1 base coin

`TradingBaseCommand.notionalToSize` (and the `%bal` path beside it) must divide
the base amount by `market.contractSize` via `baseToNativeContracts`. Both
handled **inverse** contracts from the start, and both assumed every _linear_
venue uses 1 contract = 1 base coin. That is true for bitmex / bybit / bitget —
and false for **OKX**, whose linear swaps publish a real `ctVal`:

```
DOGE-USDT-SWAP   ctVal 1000 DOGE   $10 → 142.86 base → sent as 142.86 contracts = $10,000
BTC-USDT-SWAP    ctVal 0.01 BTC    0.0001 base → adjust(_, lotSz 0.01) → 0 → rejected
```

One bug, opposite symptoms depending on which side of 1 `ctVal` falls: an order
1000× too large (rejected `51008 Insufficient USDT margin` — on a _healthy_
account) or one that rounds to zero (`must be finite and positive after
adjustment`, which is why sub-lot positions could not be closed).

**This function is shared by three runtimes** — the terminal CLI, the **web CLI**
(`apps/web/src/components/cli/CLI.tsx` imports `cli-core` and runs it in the
browser), and the worker command executor. It is not CLI-only.

`apps/web/src/order/createMarketOrder.ts` does its own equivalent division, which
is why the web **order form** was never affected. Don't "unify" by adding the
conversion inside an exchange's `placeOrder` — the web order path would then
double-convert. `BuyCommand.test.ts` now covers both linear ctVal directions.

## Token detection gotcha — `validateSymbols` rejects size-shaped strings

`TradingBaseCommand.transformSymbol` does fuzzy base-prefix matching:
if `'$1000$'.startsWith(market.base)` is true for any market, that
market is returned as the resolved symbol. This was load-bearing
when users typed half-baked tickers like `BTC` and expected the
resolver to find `BTCUSDT`.

The trap: tokens like `'1000$'` (suffix-USD size) and `'5%'` (percent)
can also accidentally prefix-match real markets. On exchanges that
list a coin with base `'1000'` (some chains do — literal "1000"
tokens exist) or base `'1'` / `'10'` / `'100'`, `'1000$'` gets
absorbed by the symbol resolver. The downstream effect was `buy
1000$` reaching the exchange with no `qty` → "orderQty is required"
errors from Bybit.

Fix is in `validateSymbols`: a `tokenLooksLikeNonSymbol` filter runs
before `transformSymbol`. Anything that already parses as a size
(`$N` / `N$` / `N%` / `N%bal` / `N%pos`), a signed/unsigned numeric,
or a relative percent (`+1%` / `-1.1%`) is rejected upfront so it
never reaches the fuzzy matcher.

If you add a new size-shaped or price-shaped token to the user-
facing surface, also extend `tokenLooksLikeNonSymbol` so the symbol
resolver can't claim it. Tests in `BuyCommand.test.ts` (`e2e: buy
1000$ when an exchange has a base="1000" market`) lock the contract
in.

## Table Formatting System

The table system provides platform-agnostic table rendering that automatically adapts to the execution environment (real terminal vs web terminal).

### Location

`packages/cli-core/src/table/Table.ts`

### Key Exports

```typescript
import {
  renderTable,
  cell,
  separatorRow,
  setTableRenderMode,
  getTableRenderMode,
  type TableData,
  type TableColumn,
  type TableRow,
  type TableCell,
  type TextStyle,
  type TableRenderMode,
} from '@tealstreet/cli-core';
```

### Usage Example

```typescript
import { renderTable, cell, type TableData } from '@tealstreet/cli-core';

const tableData: TableData = {
  columns: [
    { header: 'Status', width: 10 },
    { header: 'Exchange', width: 10 },
    { header: 'Symbol', width: 10 },
    { header: 'Count', width: 8, align: 'right' },
  ],
  rows: [
    {
      cells: [cell('Running', 'green'), cell('bybit'), cell('BTCUSDT'), cell('1,234', 'none', 'right')],
    },
    {
      cells: [cell('Stopped', 'dim'), cell('binance'), cell('ETHUSDT'), cell('-', 'dim', 'right')],
    },
  ],
  footer: 'Total: 2 recordings',
  footerStyle: 'dim',
};

// Renders based on current mode (terminal or web)
const output = renderTable(tableData);
```

### Render Modes

The system supports two render modes:

1. **Terminal Mode** (`'terminal'`): Uses box-drawing characters for borders

   ```
   ┌────────────┬────────────┬────────────┬──────────┐
   │ Status     │ Exchange   │ Symbol     │ Count    │
   ├────────────┼────────────┼────────────┼──────────┤
   │ Running    │ bybit      │ BTCUSDT    │    1,234 │
   │ Stopped    │ binance    │ ETHUSDT    │        - │
   └────────────┴────────────┴────────────┴──────────┘
   ```

2. **Web Mode** (`'web'`): Uses ASCII characters with non-breaking spaces
   ```
   Status      Exchange    Symbol      Count
   ----------  ----------  ----------  --------
   Running     bybit       BTCUSDT        1,234
   Stopped     binance     ETHUSDT            -
   ```

### Setting the Render Mode

```typescript
import { setTableRenderMode } from '@tealstreet/cli-core';

// Set at app startup based on environment
setTableRenderMode('terminal'); // For real CLI
setTableRenderMode('web'); // For web terminal
```

The mode should be set once at application startup:

- **CLI app**: Uses the default `'terminal'` mode (no explicit call needed since `'terminal'` is the default)
- **Web terminal**: Sets `'web'` mode in `apps/web/src/components/cli/webPlatformAdapter.ts`
- **Web worker (cli-core worker-host)**: Sets `'web'` mode in `packages/cli-core/src/worker-host/WorkerPlatformAdapter.ts`

### Cell Helper

The `cell()` function creates styled cells:

```typescript
cell(value: string, style?: TextStyle, align?: 'left' | 'right' | 'center'): TableCell
```

**Text Styles Available:**

- `'green'` - Green text (success, running)
- `'red'` - Red text (error, stopped)
- `'yellow'` - Yellow text (warning, paused)
- `'cyan'` - Cyan text (info)
- `'dim'` - Dimmed text (secondary info)
- `'bold'` - Bold text (emphasis)
- `'none'` - No styling (default)

### Separator Rows

Add horizontal separators between rows:

```typescript
import { separatorRow } from '@tealstreet/cli-core';

tableData.rows = [
  { cells: [cell('Header'), cell('Value')] },
  separatorRow(2), // 2 columns
  { cells: [cell('Row 1'), cell('Data')] },
];
```

### Column Configuration

```typescript
interface TableColumn {
  header: string; // Column header text
  width: number; // Fixed width in characters
  align?: 'left' | 'right' | 'center'; // Default: 'left'
}
```

### Important Notes

1. **Always use the shared table system** - Don't create custom table formatters
2. **No emojis in tables** - Emojis render incorrectly in some terminals. Use text + color styles instead:
   - Instead of `'🟢 Running'` use `cell('Running', 'green')`
   - Instead of `'⏸️ Paused'` use `cell('Paused', 'yellow')`
3. **Web mode uses non-breaking spaces** - This prevents HTML whitespace collapse
4. **Column widths are hints** - Web mode may auto-adjust based on content
