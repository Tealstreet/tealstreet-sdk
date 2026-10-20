# CLI Core Package

Shared utilities for the Tealstreet CLI and web terminal.

## Branding (`src/brand.ts`)

Runtime text that names the product — REPL `help`, `accounts` guidance, the
docs URL — reads `getCliBrand()`. The standalone CLI sets it at startup from
its build-time brand; the web CLI keeps the Tealstreet default. Never put a
product name or a `tealstreet.io` URL in a printed string directly; a
whitelabel binary greps its help for the source brand. The routing flags are
accepted as both `--proxy`/`--tunnel`/`--ws-route` and `--tealstreet-*` on
every brand; `brand.flags` only decides which spelling help shows.

## Account commands

- `accounts add` and `accounts import` must build/validate new accounts through
  `@tealstreet/safe-cex/src/accounts/buildExchangeAccountConfig` before
  persisting. Manual add supports `--proxy` (alias `--tealstreet-proxy`) / `--proxy-region` so
  standalone CLI can express the same saved account routing shape as the web
  "Use Tealstreet IPs" checkbox. Do not add CLI-local exchange credential
  schemas; derive required fields from safe-cex.

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

Native exchanges expose an optional `syncAccountState()` read seam. Connected
commands call it before selecting orders/positions or sizing. Account-dependent
preprocess constants, loops and conditions synchronize before reading the store;
conditions repeat that read per check. This is a local native-state snapshot,
not network refresh. Session management and ticker-only predicates stay usable
without a private snapshot. Read failures follow the normal visible command
error path rather than placing against a stale mirror.

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

## Every order goes through `src/order/prepareCliOrder.ts`

Placement and amend calls in `src/commands/trading/` used to hand
`exchange.placeOrder` whatever the command had computed. So `buy ... at` never
rounded its price to the tick, `stop` sent an unrounded trigger AND an
`amount: 0` infinity stop to the twelve venues that do not read one, `close`
sent a raw `Math.abs(position.contracts)`, and nothing anywhere applied the
reduce-only clamp. Web, mobile and custom scripts had all of it, through
`createCoreAdapter`.

`prepareCliOrder` calls the SAME body that adapter calls —
`@tealstreet/script-api-core`'s `prepareOrder`. It could not simply take a
`CoreAdapter`, because an adapter resolves everything through an account name
and these commands run from three hosts, two of which build a `CommandContext`
with a bare exchange handle and `accountName: undefined`. The policy was
lifted out of that closure instead; see `packages/script-api-core/CLAUDE.md`.

`cliOrdersGoThroughPolicy.test.ts` enforces it **per call, not per file** — a
per-file version passed while `CloseCommand` had one wrapped write and one raw
one, which is exactly how a file with two writes loses one of them.
`NukeCommand`, `TwapCommand` and `ChaseCommand` are allowlisted because they
hand the whole operation to a lower-level manager; their entry sizes still come
from `TradingBaseCommand`.

Three consequences worth knowing before debugging one:

- **A command now refuses rather than sending an unrounded number.** "Market
  not loaded" is precisely the state that used to produce bad numbers, so it is
  the state that has to fail. On a global `close` the refusal is caught per
  position — one symbol that is not ready must not abort the sweep.
- **Test fixtures need a REAL venue id.** `name: 'test'` answers "unknown
  venue" to every metadata question, so round-down, infinity stops and the
  clamp all go untested. The trading fixtures use `binance`.
- **`swarm` maps with an `async` callback on purpose.** `prepareCliOrder`
  throws synchronously, and a synchronous throw inside `.map` would reject the
  whole batch — the opposite of the "a single bad slice doesn't stop the rest"
  behaviour the comment above it promises.

Rust Bybit discovery carries typed instrument labels through flush frames into
the same `Market.info` fields read by `prepareCliOrder`. The native product
semantics integration gate exercises that real preflight after buffer transfer:
equity, ETF, commodity and FX refuse; empty/innovation crypto pass; absent and
new venue labels retain their unknown classification. No additional venue read
or agreement signing occurs during preflight.

## Order sizing: "linear" does NOT imply 1 contract = 1 base coin

`TradingBaseCommand.notionalToSize` (and the `%bal` path beside it) must divide
the base amount by `market.contractSize` via `baseToNativeContracts`. Both
handled **inverse** contracts from the start, and both assumed every _linear_
venue uses 1 contract = 1 base coin. That is true for bybit / bitget —
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

## An amend re-checks the price AFTER rounding

`bump` and `move` validate the shifted price before handing it to
`prepareCliAmendPrice`, which is not enough: rounding itself can produce zero.
An order at `0.001` on a `0.001` tick bumped by `-0.0008` is a valid `0.0002`,
and `roundOrderPrice(0.0002, 0.001)` is `0` — so the amend reached the venue as
`price: 0`. `prepareOrder` and `createCoreAdapter.updateOrder` both re-check;
this was the one copy that did not.

## Worker output is tagged with the request's `reqId`

`WorkerPlatformAdapter(exchange, reqId?)` and `commandRouter(…, reqId?)` stamp
every `exec-log` line with the `reqId` the browser sent, and safe-cex's
`CommandHandler` answers a THROWN command with a failed `exec-result` for that
reqId. Before this, output was untagged — anything listening on an account
(every CLI tab, the command bar) received everyone's lines — and a throw left
an awaiting caller to its 30s timeout. The command bar uses `cb:`-prefixed
reqIds and CLI tabs skip those lines; keep both halves if you change either.

## `be` answers the ENTRY price, and that is a data limit

`BreakevenCommand` calls the shared arithmetic in
`script-api-core/src/order/breakevenPrice.ts` and supplies **no `bankedPnl`**.

It was briefly wired to `Position.realizedPnl` behind a per-venue capability
flag. That was reverted, because three of the five entries were assertions and
one was contradicted by measurement already in the repo:

- **bitunix** — `docs/POSITION_MODEL_VENUE_EVIDENCE_v1.md` measures
  `realizedPNL` over a deliberately funding-bearing 90-day window and concludes
  it is net of fees AND funding. Captured `position` frames in
  `bitunix.ws-private.ts` show it equal to `fee` on a fresh open.
- **bybit** — `curRealisedPnl` appears in three mapper lines and nowhere else:
  no fixture, test, comment or doc says what it means. What evidence exists
  points the other way, since `bybit/CLAUDE.md` and the journal adapter both
  treat bybit's position-level realised figure as net of fees and funding.
- **paradex** — inferred from the field's NAME. That is not evidence.

And on venues that do report something: lifetime per symbol on asterdex,
surviving the close on lighter, funding and fees folded in on bitget,
base-denominated on every inverse contract, absent entirely on binance, okx,
blofin, kraken, woo, coinbase and most others.

**The web app does not read that field either.** It derives banked PnL and fees
from the user's own FILLS (`processTrades`), reconstructing each position
lifecycle itself. That is the only sound route. The CLI cannot take it yet — it
never calls `fetchHistoricalFills` — so entry is the honest answer and the user
docs say so.

**A claim about what a venue field MEANS needs a measurement or a citation.**
`exchangeMetadata.ts` briefly carried the comment "the name says the scope".
That sentence should have been impossible to commit.

**The offset is not side-aware.** `+` is up the price axis for both sides, so
`be +3%` locks a little in on a long and WIDENS a short's stop; the short
equivalent is `be -3%`.

**It warns when break-even lands beyond the mark.** The order then becomes a
take-profit and the stop this command just cancelled has no replacement, so
reporting only "Stop moved" would claim protection the position does not have.

**It cancels stop losses only.** `cancel stops; stop $entry` is not the same
thing — `cancel stops` means "not a limit, not a market", which takes out every
rung of a TP ladder.

## `when` — the only primitive that outlives the prompt

`when (<condition>) (<command>)` arms a command to fire ONCE, later.
`src/triggers/triggerRegistry.ts` owns the arming; `WhenCommand` owns the
parsing. It exists because `wait` cannot do this job and cannot be made to:
`wait` holds the line it is typed on and is capped at ten minutes
(`MAX_TIMEOUT_MS`), while "move my stop when TP1 fills" is a wait of hours at a
prompt the user wants to keep using.

**The parens are the syntax, not decoration.** A condition is variable-length
(`fill BTCUSDT`, `both (A) and (B)`, `@x >= 5`) and so is a command, so no
heuristic can find the split. `if (<cond>) (<then>)` solved this first in the
preprocess layer; `when` borrows its `peelParenBlock` and parses from
**`ctx.raw`** — `parseArgs` splits on whitespace and loses the grouping.

**The two halves resolve at different times, and getting it backwards is
silent.** `preprocessSegment` has a `WHEN_COMMAND` branch modelled on
`SET_COMMAND`'s "preserve the target, substitute the value": the CONDITION is
substituted at arm time, while the position giving `$entry` and `$roe101` their
meaning is the one the user is looking at; the ACTION is kept LITERAL and
re-preprocessed when it fires, because `be +3%` must read the position as it
will be then. Substituting the action would freeze it to a price computed
before TP1 ever filled. `FULL_PASSTHROUGH_COMMANDS` is the wrong lever here —
it would keep the condition literal too, and `parseConditionText` needs numbers.

The condition is substituted with `keepLiveVarsLiteral`, same as `wait`: a
condition is re-CHECKED every poll and `parseConditionText` resolves an `@x`
LHS at check time, so freezing a live var would compare against a snapshot
taken when the trigger was armed.

Firing is SERIALIZED through a module-level promise chain, and the depth is
restored rather than zeroed. Both are one bug: `firingDepth` is module state
read by a nested `when`, and two triggers firing in the same tick interleaved
across the `await` and reset each other to 0 — so a self-arming trigger could
slip past the cap meant to stop it placing orders in a loop. The host puts a
fired command on the REPL's own `commandQueue` for the related reason: a
trigger fires from a timer, and without that it interleaves with a chain the
user is midway through.

The branch runs **after alias expansion**, at the same point `wait`'s
`keepLiveVarsLiteral` check does and for the same reason: reached through an
alias the head is the ALIAS, so a check placed earlier misses it entirely and
an aliased `when` gets both halves substituted — freezing the action to a price
computed before the trigger ever fired.

**It fires through the HOST, not through `executeCommand`.**
`CLIPlatformAdapter.runChain` is optional and its absence is load-bearing.
`executeCommand` does no preprocessing — that lives in `repl.ts`'s
`processCommand`, along with alias expansion, chain operators, the runner
failure contract, `recordWriteOp` and `taskAction` dispatch. A trigger firing
`chase` through the raw executor would return a `taskAction` nothing acts on,
and its orders would never reach the audit log. `repl.ts` assigns `runChain`
inside `startRepl` and NOT at module scope, because `runSingleCommand` shares
the same adapter and `teardownOneShot` calls `process.exit` — a trigger armed
one-shot could never fire, so `when` reads the missing hook and refuses.

**A fired command is SCOPED to where it was armed.** `runChain` takes an
optional `{ symbol, account }` and the REPL applies it through
`runWithAdapterContext` — the same per-call override the listener's `cmd:exec`
uses, which scopes a run without mutating the session the user is still typing
into. Without it a trigger armed on BTCUSDT would fire `be +3%` against
whatever symbol the session had drifted to, and move the wrong position's stop.
That override's `printSink` is now optional for this: a context can exist purely
to scope a run, and the fired command's output belongs on the terminal like any
other, so consumers guard on `ctx?.printSink` rather than on `ctx`.

**The exchange is re-resolved at fire time**, never closed over: switching
account calls `stopExchange` on the old instance, so a captured reference would
poll a dead store and send its order to an account the user has left. A null
exchange keeps polling rather than settling — an account mid-reconnect is not a
reason to stop protecting a position.

Three limits, each with a reason: it fires **once** and disarms; it **expires
after 12h**, because an invisible trigger that can live forever is one that
fires a market order next week; and it refuses past **5 levels of nesting** — a
trigger arming its next rung is a ladder, a trigger arming itself is a loop
that places orders.

**Triggers are listed FIRST by `listTasks`.** Bare `kill` takes the last active
row, which has always meant the chaser or TWAP just started; appending would
move that target onto an armed `when`. `TaskRow.side` is optional for the same
honesty reason — a trigger has no side, and reading one out of command text
would be a guess.

The poller is a self-rescheduling `setTimeout` with `unref()`, never
`setInterval` (root CLAUDE.md). The `unref` is not a nicety: a ref'd timer here
reproduces the `logger.ts` bug users reported as "the CLI never exits".


## `buy` / `sell`: two parser traps that both routed real orders wrong

**Any leftover bare number refuses, not just one >= 100.** The forgotten-`at`
guard in `BaseBuySellCommand` used to fire only above 100, a threshold that
silently assumed BTC-shaped prices. On a coin quoted in cents it never fired,
so `sell 33$ 0.051533 reduce` DROPPED the price and market-sold — reported by a
user as take-profits filling instantly at a terrible price. No reading of an
unconsumed number here means anything, so the magnitude test is gone.

**The limit path skips `stop` in its trailing-symbol scan.** The market and
`best` paths already did; this one skipped only `reduce` and `po`. So
`buy 100$ at 0.051124 stop 0.049570` walked back from the end, hit the literal
token `stop`, and `resolveMarketSymbol` prefix-matched `STOP` against any base
a venue lists — reproduced in a test as an entire bracket placed on `SUSDT`
instead of `BTCUSDT`. Note the reproduction needs a TICKER, not just a market:
`validateSymbol` checks `store.tickers`, which is why the default fixture hid
this.

## Structured command failures

`executeCommand` preserves actual thrown `Error` attribution as optional
`ExecuteResult.errorDetails`, using safe-cex's `src/commandOutcome` allowlist.
The worker router forwards it alongside the existing error string and
`errorReported` flag. Interrupted native writes may carry caller client IDs and
`outcome: 'unknown'`; known partial venue rejections retain accepted order IDs
without becoming unknown outcomes. Neither serialization nor timeout authorizes
a retry or reconstructs missing native-generated IDs.
The outer executor always prints a thrown failure as the command's result, even
when safe-cex already emitted it as an exchange `'error'`: that listener filters
suppressed categories, shows auth failures once per account and dedupes repeats,
so it cannot guarantee the user sees a re-run rejection. The worker router skips
its own extra `exec-log` because the executor line already reached the stream.

This outer-executor contract covers errors that propagate from awaited order
calls. Commands which aggregate or log errors internally (global close, swarm,
and fire-and-forget nuke) keep their existing result semantics; the outer
executor cannot recover metadata those command implementations have discarded.
