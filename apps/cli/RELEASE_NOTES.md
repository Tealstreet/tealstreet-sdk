# Release Notes

Each release gets its own `## vX.Y.Z — YYYY-MM-DD` section. The release
script extracts the section matching the current `package.json` version
and uses it as the GitHub release body — so put the version heading
above the new bullet points before running `yarn release`.

## v0.9.2 — 2026-05-25

Adds a customizable bottom status footer.

### Customizable status footer

- **A persistent footer pinned to the bottom of the terminal**, above
  which your prompt and output live. Shows always-on, glanceable data and
  updates in place — a companion to the transient top alert bar. On by
  default; `footer off` opts out.
- **Built-in segments**: connection, account, symbol, balance, focused-
  symbol uPnL ($ + %), total uPnL, position, mark / last price, funding,
  clock, CLI version, and literal text. Arrange them with `footer add` /
  `footer remove` / `footer reset`, split across left/right groups, and
  set per-segment colours in `config.json`. All read already-streamed
  data — no extra API calls.
- **Custom-script segments**: `footer add script <name>` renders the
  return value of one of your scripts (~once a second). These runs are
  hard read-only — a footer can never place or cancel orders — silent,
  and time-boxed, so a slow or misbehaving script can't stall or trade.
- **The CLI version moved out of the prompt prefix** into the (removable)
  `version` footer segment, so the prompt itself is cleaner.

## v0.9.1 — 2026-05-24

Two releases in one: the execution-hardening batch that was staged for
v0.9.0 (never shipped) plus a round of terminal-UX polish on top. If
you're coming from v0.8.1, everything below is new.

### Terminal output & UX polish

- **The terminal stays clean.** Operational WebSocket chatter —
  keepalive pings, reconnect notices, balance-stream dumps — no longer
  prints to the prompt. It all still lands in the log file; run
  `tealstreet logs` (or `yarn logs`) to tail it. A startup hint points
  you there.
- **Logs no longer eat your typing.** Async log lines that do surface now
  print _above_ the prompt and redraw your in-progress input intact,
  instead of corrupting the line you're editing.
- **Pinned status bar.** A reserved top line mirrors the latest notable
  event — warnings, errors, and critical events — with color coding,
  and dims a few seconds after it lands so it stays glanceable without
  nagging. Critical events (auth failures, liquidations) read loud.
- **Honest connection state in the prompt.** The prompt now reflects the
  real connection/auth state instead of optimistically showing an
  account as connected before the link actually settled.
- **Custom scripts with no account restriction now just work.** A script
  declaring an empty `accounts` list runs against your active account
  instead of erroring out.
- **Dev/version reporting fixed.** Builds report their real version
  (previously some paths showed `v0.1.0`).

### Safety & correctness fixes

- **Bad explicit symbol now fails closed.** An unknown symbol token (e.g.
  a typo) is rejected instead of silently falling back to the focused
  symbol — so an order can't land on the wrong market. Applies to
  comma-separated symbol lists too: one bad symbol rejects the whole
  command.
- **`cancel @ids` can no longer widen into a cancel-all.** If a captured
  `@var` holds zero order IDs, the cancel step is skipped with a clear
  message rather than falling through to a server-side
  cancel-everything-on-this-symbol. On BitMEX, stale/phantom orders no
  longer linger after a failed fetch.
- **CLI TWAP fixes.** `reduceOnly` is honored on task-start, the task runs
  against the per-command account/symbol override (not whatever the REPL
  last switched to), and failed slices surface as errors instead of being
  swallowed.
- **Sizing fixes.** Reduce-by-percent now sizes off your **position**, not
  your balance; a zero-size guard blocks empty orders; quanto/inverse
  `max` display is corrected.

### Live-action safety rails

- **`;` now waits, `&` runs in parallel.** A `;`-separated step waits for
  all prior in-flight steps to finish before running — so a sequence like
  `close all` then `buy $1000` buys only after the close completes. `&`
  keeps steps parallel. The old `@cmd` parallel prefix still works but
  warns once — migrate to `&`.
- **Degraded-connection write lock.** When the connection is unhealthy,
  exposure-increasing orders are blocked while close / cancel / reduce
  stay allowed. Fails open — it never blocks on an unknown state.
- **New `budget` command — opt-in notional cap.** Set a session cap with a
  USD amount (e.g. `budget $5000`); bare `budget` shows the current cap
  and spend, `budget off` disables it, and `budget reset` zeroes the spend
  counter. Off by default. The cap persists across sessions; spend is
  session-only and is counted only after an order actually places.

### New `audit` command — durable write-op log

- **`audit on` starts a durable, append-only log** of every order /
  cancel / close / leverage command — including failures — at
  `~/.tealstreet/audit.log`. Off by default; `audit status` and
  `audit tail [N]` inspect it. Reads (positions, orders, etc.) are never
  logged.
- **Scripts are covered.** Orders placed inside a `script:<name>` run are
  logged too (tagged `[script]`), alongside the named invocation.
- **Everything from one command shares a run ID.** All orders from a
  `repeat 10` or a single script run group under one `run=<id>`.
- **Fills are attributed.** When a tracked order fills, the log records it
  with the same run ID and a pointer back to the command that placed it —
  so you can see what actually executed, not just what was sent.
- **Bounded on disk.** The log is size-capped (~5 MB) and rolls to a
  single backup; `audit tail` reads from the end.

## v0.8.1 — 2026-05-17

### Listener: post-bind client-hint timer

- After `Bun.serve` binds the loopback listener, the CLI now sets a
  **75 s one-shot timer** that prints a "refresh the tab" hint if no
  web client has connected by then. Companion change in the web app
  doubled the loopback-reconnect ceiling from 30 s → 60 s, so the
  75 s timer sits just above the worst-case web reconnect window.
- Timer is cleared on first WS open and on `stopListener`.
  Origin-blocked WS opens don't suppress the hint — only legitimately
  connecting clients do.
- The new `/cli/operations/listener.md` doc references this behavior;
  this release brings the binary in line with the docs.

### Docs

- CLI documentation rewritten from scratch and promoted to its own
  Docusaurus section at [`docs.tealstreet.io/cli`](https://docs.tealstreet.io/cli)
  (separate sidebar + search index, dedicated navbar entry). The
  old `/docs/cli/*` URLs redirect to the new layout.

## v0.8.0 — 2026-05-16

### Loopback CLI listener (replaces `serve` mode)

- **The CLI binds a loopback WebSocket + HTTP server on
  `127.0.0.1:53219` whenever the REPL is running.** The Tealstreet
  web app's "Local CLI" tab detects it automatically; one-time
  approval in the REPL pairs the two sides for good. Removes the
  old `tealstreet pair` / `unpair` / `serve` commands — no manual
  pairing step, no relay round-trip.
- **Bidirectional account sync.** Add or edit an account on either
  side (REPL `account add` or the web settings panel) and the
  other side picks it up immediately. Conflicts resolved by
  last-write-wins on each account's `updatedAt`. Deletes propagate
  via tombstones; tombstones GC after 30 days.
- **Web commands scope to the web tab.** Type `buy 100` in the
  web's CLI tab and it runs against the **tab's selected account +
  symbol**, not whatever the local REPL last switched to. The REPL
  session is preserved on the override.

### Webhooks

- **`hook list` / `hook create <name> "<command>" [--account <name>]`
  / `hook revoke` / `hook rotate` / `hook show`.** Register
  webhooks that fire commands via authenticated `POST /hook/<id>`
  from external services (TradingView alerts, GitHub Actions, cron).
  Each hook gets a 32-hex secret passed as `X-Hook-Token`.
- **`hook --account <name>` actually scopes the run** (previously
  metadata-only — the configured account is now threaded into the
  command's execution context on fire).

### Trusted-client management

- **`clients list` / `clients revoke <id|prefix|--all>`.** Manage
  which web clients the CLI has approved. Prefix matching requires
  ≥ 8 chars or full id; ambiguous prefixes list the matches.

### Kill switches

- **Three knobs in `~/.tealstreet/config.json` under `listener.*`:**
  `enabled` (port bind), `accountSync` (account broadcasts +
  inbound writes), `remoteExec` (`cmd:exec` + webhook fire). All
  default to `true`. Reads on every call — flip the config without
  restarting.

### Quality-of-life fixes

- **`watch position` / `watch order` / `watch balance` (singular
  forms) now work** alongside the existing `wa`/`wp`/`wo`/`wm`
  short aliases and the plural long forms.
- **`account remove <typo>` returns "not found"** instead of
  silently creating a junk tombstone for the non-existent name.
- **Listener-approval prompt timeout no longer absorbs your next
  REPL command.**

### Documentation

- **User docs at [docs.tealstreet.io](https://docs.tealstreet.io)**
  with install / quickstart / concepts / cookbook / vars & aliases /
  full commands reference / listener wire spec / pairing model /
  recording / troubleshooting.

## v0.7.0 — 2026-05-10

### Features

- **`$entry +N%` inline percent offset.** `at $entry +1%` and
  `at $entry -0.5%` now resolve to a single absolute price token
  (anchor + anchor × pct/100) before the command parser sees them.
  Same shape works on `$lentry` and `$sentry` in hedge mode.
  Absolute offsets (`+500`, `-100`) keep working as before. Useful
  for "TP at 1% above entry" without precomputing.
- **Multi-statement alias bodies splice back into the chain.**
  `alias pair "buy 1$ btcusdt; buy 1$ ethusdt"` followed by `pair`
  now produces two chain steps, not one segment with an embedded
  `;`. Works with `&` separators, nested aliases, `repeat`, and
  preserves `sleep` block flags. Quoted bodies (e.g. `print "a; b"`)
  stay opaque.
- **Ticker price-anchor constants.** `$bid`, `$ask`, `$last`, `$mark`,
  `$index` resolve from the focused symbol's ticker. All accept the
  same inline `+N` / `+N%` / `-N` / `-N%` offset combining as
  `$entry`, so `buy 0.1 at $bid +5` and `sell 0.1 at $last -0.1%`
  collapse to a single price token before dispatch.
- **Account / PnL substitution constants.** Ten new numeric
  constants — `$free`, `$used`, `$total`, `$upnl` (balance fields);
  `$pnl` (focused-symbol unrealized PnL); `$totalupnl` (all
  positions); `$totallongusd`, `$totalshortusd` (notional sums);
  `$totallong`, `$totalshort` (notional / total ratios). Useful in
  prints, alias bodies, and `set @x` snapshots.
- **Cancel filter expansion.** `cancel ro` / `cancel xro` (reduce-
  only / non-reduce-only), `cancel buys ro` / `cancel sells xro`,
  `cancel from <p1> to <p2>` (price-range), `cancel limits` (skip
  stops), and `cancel <id-or-suffix>` (by full ID or trailing 3+
  chars). Symbol-scope still wins: `cancel BTCUSDT` is a symbol
  cancel even if an order ID happens to end in `BTCUSDT`.
- **Global close variants.** `close longs` / `close shorts` /
  `close allpos` / `close all` close every matching position with
  reduce-only market orders. `close longs <size>` partial-closes
  every long with the same size on each leg. Skips zero-contract
  positions. Reports `(no shorts positions to close)` rather than
  firing zero orders silently.
- **`loop <variant>: <body>` fan-out across positions.** New
  preprocess primitive — `loop longs: stop sell at $entry -2%`
  emits one step per long position with the symbol appended.
  Variants: `longs`, `shorts`, `openpos` / `open`, `allpos` / `all`.
  Aliasable, composes with `repeat`, and respects the chain.
- **Symbol groups via `@var`.** `set @grp BTCUSDT,ETHUSDT,SOLUSDT`
  and then `buy 1$ @grp` fans out across the group. Already worked
  via the existing variable substitution; now documented as a
  first-class pattern in `vars-and-aliases.md`.
- **Aliases `fuck` and `terminate`** for `kill` (ichibot muscle
  memory). `kill last` / `fuck last` / `terminate last` all map to
  "kill the most-recent active task."
- **`vars` / `vars list`** as alias for `print all`. `vars @x` shows
  one variable.
- **Within-token math expressions.** `100*@mul`, `$entry-100`,
  `$ask-$bid`, `$totallongusd/$total` — any single token of the
  form `<operand><op><operand>(<op><operand>)*` evaluates at
  substitution time. Operands are float literals, `$constants`,
  or `@vars`. Operators `+ - * /` with standard precedence.
  Symbols / size tokens (`BTCUSDT/USDT`, `100$`, `5%`) pass through
  unchanged because they don't match the operand-shaped chunks on
  both sides of the operator.
- **`whitelist` / `unwhitelist`** — persistent symbol set that
  `close longs` / `close shorts` / `close allpos` / `close all`
  refuse to touch. Symbol-scoped closes still fire (the gate only
  protects the global form). `nuke` does NOT honour the whitelist
  — it talks to exchange bulk-close primitives directly.
- **`fatfinger <contracts> [<symbol>]` / `unfatfinger`** —
  per-symbol max-order-size guard. Sized commands (buy / sell /
  scale / swarm / twap / chase) reject any individual order whose
  computed amount exceeds the limit. Reduce-only orders skip the
  check (closing exposure is fine). Per-individual matching means
  `buy 1, repeat 11` is allowed even if the total exceeds the
  limit — same as ichibot.
- **`simulation on/off` (aliases `sim`, `dry-run`)** — global
  dry-run flag. When on, write-side trading commands log
  `[sim] <command>` and skip the exchange call. Read commands
  (`positions`, `orders`, `margin`, etc.) still work so you can
  inspect state. Persistent across reloads.
- **`retry <N> <command>`** — runner awaits the step and retries up
  to N times on error (cap N=10). Pairs with `set ec` to make a
  retry-exhausted step a soft warning instead of a chain-stopper.
- **`set ec` (chain modifier)** — flip continue-on-error for the
  rest of the chain. Errors on subsequent steps print as
  `(continue-on-error) <msg>` warnings instead of red errors.
  Doesn't persist across separate command invocations.
- **`set ro` / `set po` (+ `unset ro` / `unset po`) chain modifiers** —
  flip reduce-only / post-only on subsequent order-placing steps.
  `set ro` injects the `reduce` keyword into `buy`/`sell`/`scale`/
  `swarm`/`chase`/`twap`. `set po` injects `po` into `buy`/`sell`
  limit orders (market orders ignore it). `close` and `stop` are
  reduce-only by construction and aren't touched. Idempotent.
- **`po` keyword on buy/sell limit orders** — `buy $100 at 49000 po`
  sets `timeInForce: PostOnly`. Works with `best` and pairs with
  `reduce`.
- **`wait <condition> [timeout=<dur>] [for=<dur>]`** — block the chain until a
  condition holds. Conditions:
  - Duration: `wait <duration>` (sleep alias), `wait time <HHMM>` (next UTC clock time).
  - Position: `wait position [SYM]` / `wait anyposition` (any symbol) /
    `wait long [SYM]` / `wait short [SYM]`.
  - Price: `wait price <op> <n> [SYM]` — side-aware (`<` reads bid,
    `>` reads ask). Also `wait mark <op> <n>`, `wait index <op> <n>`.
  - PnL: `wait pnl <op> <n> [SYM]`.
  - Derivatives: `wait fundingrate <op> <n>`, `wait premium <op> <n>`.
  - Order state: `wait fill [SYM]`.
  - Combinators: `wait either (<A>) or (<B>) [or (<C>) …]`,
    `wait both (<A>) and (<B>) [and (<C>) …]`. Nest freely.
  - Modifiers: `timeout=<dur>` (default 30s, max 10m),
    `for=<dur>` (condition must hold continuously — anti-fakeout).
    Polls every 100ms. Combinators + `for=` compose; combinator
    vocabulary is shared with `if (...) (then)`.
- **`if (<cond>) (<then>) [else (<else>)]`** — preprocess-time
  conditional. Same condition vocabulary as `wait`. Multi-statement
  branches via `;` or `&` inside the parens. Composes with
  `repeat`, `set ec`, `retry`, nested `if`. One-shot, not reactive.
- **`==` and `!=` comparators** added to `wait` / `if` conditions.
  Useful for integer-ish values (`pnl == 0`, `possize == 0`); on
  prices the live value drifts in the last decimal so prefer
  `<` / `>`.
- **`end` marker** — early-terminate the current chain. Pairs with
  `if`: `if (cond) (end); buy 1$` drops the trailing `buy 1$` when
  `cond` is true. Propagates through nested ifs and through
  `repeat` (cuts the rest of the iterations on first hit). Aliasable
  — `alias bail end`.
- **Reactive variables — `sets @<name> <source>`.** Symmetric to
  `set` but stores the SOURCE expression (not a snapshot) so the
  value re-evaluates on every `@<name>` use. Source can be any
  `$constant`, `@var`, numeric literal, math expression
  (`$ask-$bid`, `@mul*2`), or anchor + offset (`$entry +1%`).
  Garbage rejected at definition time; semantic errors surface at
  first use. Session-scoped (not persisted) and mutually exclusive
  with `set` on the same name. Cycles between live vars
  (`sets @a @b; sets @b @a`) are detected and reported with the
  chain.
  - Conditions accept `@<name>` as a numeric LHS:
    `wait @x > 25` re-evaluates the source on every poll;
    `if (@spread < 5) (close)` evaluates once. Static @-vars also
    work in both, comparing against the frozen value.
  - `print @x` shows live vars as `@x (live) = <source>  →  <value>`
    with the current evaluation inline. Evaluation errors render
    as `→ (<msg>)` so the listing always completes.
  - `unset @x` clears either kind. `unset all` wipes both.
- **Tracked aggregators — `track max @<name> <source>` /
  `track min @<name> <source>`.** Persistent high-water / low-water
  mark of any source expression. Each `@<name>` use re-evaluates the
  source and updates the stored extreme. `track list` prints every
  registered tracker; `untrack @<name>` / `untrack all` remove
  them; `unset @<name>` is kind-agnostic. Three-way mutex with
  `set` and `sets`. Conditions read trackers per-tick so
  `wait @hi > 1000` releases when the high-water mark crosses
  1000 — useful for "did this ever happen" patterns. Source
  vocabulary identical to `sets`. Session-scoped (not persisted).
- **`history` / `history N`.** Print recent typed commands
  (newest first, default 50, max 200). Source is per-runtime —
  standalone CLI reads readline's history (already persisted
  to `~/.tealstreet/cli_history`); web terminal reads the
  per-tab atom. No timestamps for v1.
- **`position show [<sym>]`.** ichibot-parity keyword — same
  output as the existing `position` / `p` / `pos` (per-symbol
  focused card with side / size / entry → mark / uPnL ($+%) /
  liq / leverage). The `show` is a no-op modifier; both shapes
  produce the same card.
- **`say` as an alias for `echo`.** Substitution already runs
  on echo args, so `say "peak pnl is @hi"` works the same way;
  the alias is for ichibot muscle memory.
- **`wait fill <id-or-list>`.** Existing `wait fill [SYM]`
  (symbol-scope, today) gained a sibling that takes a specific
  order ID or a comma-list of IDs. Releases when EVERY matched
  order is no longer live (filled or cancelled). Pairs with the
  capture syntax — `@bracket = buy $100 at 49000 stop 48000; wait
fill @bracket; close` blocks until the whole bracket has
  settled. Matching is exact-id OR trailing-suffix (same shape
  as `cancel <id>`); known symbols still take precedence over
  the ID interpretation for backwards compat.
- **`@<name>` on the RHS of conditions.** Every numeric-RHS
  condition (`price > N`, `pnl < N`, `mark`, `index`,
  `fundingrate`, `premium`, `@x > N`) now also accepts
  `@<name>` on the right side. The RHS resolves at check time
  through the same precedence as the LHS (tracker → live →
  static). Composes with trackers + live vars to make the
  full trailing-stop pattern work in one wait:
  ```
  track max @hi $pnl
  sets @stop @hi-1
  wait pnl < @stop      # @stop re-resolves each poll → ratchets @hi
  ```
  Math on the RHS still freezes (`wait pnl < @hi-1`) — hop
  through a `sets` indirection as above.
- **Order-ID capture — `@<name> = <body>`.** Prefix any order-
  placing command (buy / sell / scale / swarm — incl. bracket
  orders and multi-symbol fan-out) to bind the resulting exchange
  order ID(s) into a user var. The runner awaits the command,
  joins every returned ID with `,`, and writes the list into
  `@<name>`. `cancel @<name>` later fans the comma-list out via
  the existing var-substitution path — cancel now matches every
  piece via exact-or-suffix and cancels everything matched in one
  call. Use cases:
  - `@o1 = buy $100 at 49000 stop 48000` → `@o1` holds entry-id +
    sl-id (and tp-id on venues that expose it).
  - `@grp = buy BTCUSDT,ETHUSDT $100` → `@grp` holds both leg IDs.
  - `@swarm = swarm buy 1000$ into 5` → only successfully-placed
    slice IDs land in `@swarm`; failed slices contribute nothing.
  - Composes with `retry N @<name> = <body>` and `set ec`.
  - **Single-prompt forward references work**: `@o1 = buy $100;
cancel @o1` resolves the cancel's `@o1` after the buy's capture
    writeback. The preprocessor leaves the reference literal and
    the runner finalizes at dispatch time. Math expressions on
    capture targets (`@o1*2`) are rejected — capture vars hold a
    comma-list, not a number.
  - Web terminal: capture now works end-to-end. The webworker
    replies to the browser CLI via an `exec-result` event echoing
    a `reqId`, and the CLI awaits it before continuing the chain.
    Same writeback + warning shapes as standalone (`@o1 =
<orderId>`, "captured 0 order IDs" on empty-array returns).
  - Limitations (documented in `apps/documentation/cli/reference/commands.md`):
    Bybit's position-level TP/SL returns no IDs and the runner
    warns.

### Internal

- Expanded cli-core test coverage (full-pipeline integration tests,
  Stop / Scale / Leverage / Max / Nuke / Tasks / Kill / Move / Echo
  / Margin / Price commands, multi-symbol fan-out for buy/sell,
  e2e regression coverage for suffix-USD sizing).
