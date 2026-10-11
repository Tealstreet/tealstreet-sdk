# `@tealstreet/oems-core`

The narrow OEMS slice. It names the order-entry/management **methods of the
shared SDK** a caller may use when it is **acting on the user's behalf**, and
it implements none of them.

## It is a SLICE OF ONE SURFACE, not a surface of its own

This package used to name the browser module-sandbox's `orders.*` RPC verbs.
That made it a fourth order vocabulary in a repo that already had three —
`ModuleAPI['orders']`, the sandbox `RPC_VERBS`, and the SDK's
`api.exchange.*` — and only the last of those crosses runtimes. Custom
scripts, hotkeys and the CLI all execute against it, so it is the one that
wins.

`OEMS_WRITE_METHODS` and `OEMS_READ_METHODS` are now **derived** from
`OEMS_WRITE_API_PATHS` / `OEMS_READ_API_PATHS` in
`packages/script-api-core/src/runtime/runtimeSurface.ts`, by stripping the
`api.` prefix. There is exactly one place the slice is decided, and it is in
the SDK.

**Do not add an `oems` entry to `RUNTIME_SURFACES`.** That record is keyed by
`TargetName`, whose members feed exhaustive `Record<TargetName, …>` tables in
`runtime/targetCheck.ts` and the `RunnableTargetName` exclusion. OEMS is a
capability narrowing, not a place code runs; conflating them makes every
target-shaped table grow a member that can never be a target.

The package exists because one consumer pays a per-request price for the width
of its API: an LLM planner spends context describing the surface before it can
use it. Narrowing is the feature.

**This slice mechanism is the intended home for every future agent surface** —
including chained read tool-calls, which want their own `AGENT_READ_API_PATHS`
here rather than a tool registry of their own. It is also the only narrowing
system the repo should have: MCP must not grow a second one. The reasoning, the
three surfaces that already drifted, and the unimplemented `manifest.json` that
caused it are in
[`docs/plans/SDK_SURFACE_VISION_v1.md`](../../docs/plans/SDK_SURFACE_VISION_v1.md).

## `readTools.ts` — the read half, projected from the generated manifest

`AGENT_READ_TOOLS` filters `CORE_READ_MANIFEST` by `AGENT_READ_API_PATHS`.
Nothing here restates the surface; adding a read capability in
`script-api-core/src/runtime/readBindings.ts` and regenerating is the whole
edit.

**These are NOT OpenAI function-tool descriptors, and must not become them.**
`OpenAiJsonClient` posts to the Responses API with
`text.format: { type: 'json_schema', strict: true }`, one `instructions`
string and one `input` string. It has no `tools` parameter and no multi-turn
input array. A `{ type: 'function', … }` descriptor would target an API this
codebase cannot call. A tool call is structured output like everything else:
the surface is described in the prompt by `buildReadToolsBlock`, and the model
asks for one by filling in `READ_CALL_SCHEMA`.

**`READ_CALL_SCHEMA` is flat and DERIVED.** Flat for the reason `planSchema.ts`
sets out at length — strict mode has no "required given `tool`", so `anyOf`
arms buy documentation rather than validation. Derived because a union of every
tool's parameters is exactly the kind of thing that rots when hand-maintained.
`buildReadCallProperties` throws at module load if two tools ever give one
field name two different base types, which is the one way a derived flat union
goes quietly wrong.

**`parseReadCalls`'s account check is the ONLY thing bounding which accounts a
read touches.** The write path is backstopped by `resolvePlan` looking every
address up in the context before a command exists. A read has no such lookup:
without this check, a model naming an account outside the snapshot would still
execute against live credentials for it. `prompt.ts`'s "use ONLY the accounts
in the snapshot" is a request; this is the enforcement. Do not move it into the
executor and do not make it a warning.

**It is only as good as the context the caller hands it.** The check says "in
the snapshot", and that means the snapshot the MODEL SAW — the bounded one.
`useAiOrders` passed the raw context until 2026-09-02, so the allowlist was
wider than the prompt by every account past `OEMS_CONTEXT_LIMITS.accounts`.
Two holes closed with it: an omitted `accounts` list is substituted with the
snapshot's names rather than passed through as null (`getCombinedBalance` reads
null as "every streaming account"), and the web executor scopes the three reads
that answer from CONFIGURATION — `getAccounts`, `getActiveAccounts` and
`getCombinedBalance` — none of which `parseReadCalls` can allowlist from its
arguments.

**Narrowing a read makes its answer partial, and a partial answer must say so.**
`getCombinedBalance` narrowed to the shown accounts returns a sum short by
however many the snapshot dropped, and offered as the total that is a worse
answer than the hole the narrowing closed. It is marked truncated instead.

**Rule 5d is what makes any marker obeyable.** `[TRUNCATED]` and
`[SNAPSHOT TRUNCATED]` are both rendered INSIDE the nonce fence, and the
fence's own preamble tells the model that instructions in there are an attack.
Without a rule in the cached instructions naming them — and saying they count
only immediately after the data they describe — the marker is a line the model
has been told to disregard.

It also drops fields the named tool does not declare — in a flat union every
field is present on every call, so a model filling `account` on a tool that
takes `accounts` would otherwise reach the binding as an unexpected key — and
caps fan-out at `MAX_READ_CALLS_PER_TURN`, because `getCandles` and
`getHistoricalPosition` are `network` cost and an uncapped list is an uncapped
number of venue round trips per turn.

**`AGENT_READ_API_PATHS` equals `CORE_READ_API_PATHS`, on purpose.** The tests
in `surface.test.ts` assert STRICT NARROWING for `OEMS_READ_METHODS` and
EQUALITY for the agent slice; they pull in opposite directions and both are
deliberate. The agent is bounded on writes plus a human confirmation. Narrowing
its reads would delete the analysis inputs the chained-read work exists to add.

## `reads` is ADDITIVE on the plan schema, not a second response type

A model that needs data it was not given asks for it in `reads` rather than
refusing. The obvious alternative — a discriminated union of "plan" and "reads"
responses — hits the same strict-mode limitation as an `anyOf` step: the model
must pick a branch before it can fill anything in. One schema with both arrays
present, either possibly empty, is what models comply with most reliably, and
it is the shape the rest of `planSchema.ts` already uses.

**A plan carrying BOTH reads and steps has its steps dropped.** Those steps
were computed without the data the same response admits it needs; executing
them would be acting on the model's own statement that it was reasoning with
something missing. `parseOemsPlan` drops them, the reply survives, and the next
hop proposes again with the answer in hand. Do not "fix" this by executing both.

`OemsPlan.reads` stays RAW. Validating one needs the context snapshot, which
`parseOemsPlan` does not take; `parseReadCalls` does it at the point of use, in
the browser about to run them against live credentials.

## The tool list is CACHED; the tool results are UNTRUSTED

`buildOemsPrompt` puts the read surface in `instructions` — our text, stable
across requests, and therefore the half that caches. It puts read RESULTS
inside the nonce fence in `input`, with the snapshot and the transcript,
because a symbol or an error string coming back from a venue is exactly as
untrusted as an account name. `prompt.test.ts` asserts the fence position by
index; if that test ever looks pedantic, it is the one stopping venue data from
being read as instructions.

Both `OEMS_PROMPT_VERSION` and `OEMS_PLAN_SCHEMA_VERSION` are part of
`prompt_cache_key`. Bump on any edit to either.

**A truncated result is MARKED, and rule 5c tells the model what the mark
means.** `renderReadResults` appends `[TRUNCATED: showing 200 of 900 …]` to a
successful result whose bounding dropped something. Both counts are gated on
`Number.isFinite`, because `readResults` arrives as JSON from the browser and
nothing on the way in checks their type — an unusable count degrades to the
generic marker rather than rendering `[object Object]`.

This is a fix, and the shape of the bug is worth keeping: truncation was
recorded on `error`, `renderReadResults` prints `error` only on the failure
arm, so the note was computed in two places and read in none. Ask for four
hours of candles, get two hundred minutes, and the model would answer about
the period you asked for. The lesson generalises past this field — the tests
that missed it asserted on the result OBJECT, and the only assertion that could
have caught it is one against the rendered prompt string.

## It is source-only, like every other package here

`main` points at `src/index.ts` and `build` is an `echo`. `apps/web` reaches it
through the workspace dependency plus the `paths` entries in
`apps/web/tsconfig.json` — both are needed, and a new consumer app needs its own
pair. There is no dist and nothing to build.

## The drift tests are the point

`src/surface.test.ts` asserts the slice is a subset of `CORE_WRITE_API_PATHS`
and `CORE_READ_API_PATHS`; `apps/web/src/components/agent/oemsSurface.test.ts`
asserts every method is a real key of the live web `CoreAdapter` (that half
must live in `apps/web`, because the adapter implementation does). Without
them a method dropped upstream would keep being described here and fail at the
point of trade.

`@tealstreet/script-api-core` is a real **dependency** now, not a test-only
one. Its capability types import `ExchangeName` from safe-cex, which is why
`tsconfig.json` aliases `@tealstreet/safe-cex/src/*` too — a type-only import,
so nothing of safe-cex loads at runtime. `vitest.config.ts` re-enables those
aliases with `resolve.tsconfigPaths`; both are load-bearing, and dropping
either breaks the test at resolution rather than at assertion.

## Why three writes are excluded

`setPositionMode` and `setMarginType` reconfigure an **account** rather than
work an order. `runCommand` is the REPL escape hatch — a planner that can
reach it can reach every verb this slice was drawn to exclude, including the
long-running ones.

The long-running verbs are absent for a different reason: they are not on this
surface at all. `placeScaledOrder`, `placeTWAPOrder`, `chaseOrder` and
`setBreakeven` are module-sandbox verbs, and the SDK expresses a ladder as
`placeOrders`. Bringing TWAP or chase to an agent needs a task + cancel
surface first: a confirm step that returns before the trading starts is not a
confirm step.

## The rounding arithmetic is NOT here any more

`rounding.ts` moved to `@tealstreet/script-api-core/src/order/rounding`. It had
to: the order POLICY moved there so every host obeys one copy, and policy needs
safe-cex types this package refuses to import. Keeping the arithmetic here
while the policy left would have closed a dependency cycle.

**The barrel does NOT re-export it**, deliberately: `roundMarketOrderSize`
names two different functions — the step-based one in `rounding.ts` and the
`Market`-shaped one in `orderPricing.ts` — and a re-export here would make both
reachable by two routes under one name. Callers import from
`@tealstreet/script-api-core/src/order/rounding` directly.

## The context types are re-declared on purpose

`src/context.types.ts` does not import safe-cex. Three reasons, in order of how
much they carry: a field-named projection **cannot leak a credential** the way
a spread-and-delete one can (account rows hold `apiKey`, `apiSecret`,
`passphrase`, `secondary_*` and wallet keys); venue `info` payloads are worth
nothing to a planner and cost the most tokens; and these names are what a
stored plan is written against, so they must be free to stay still while
safe-cex's types move.

`markets` carries only the symbols the rest of the context mentions. The full
market list is tens of thousands of rows.

## A ref is an ADDRESS, never a payload

None of these refs is a whole safe-cex `Order` or `Position`, deliberately.
The adapter re-resolves an address against the host's own store and forwards
ITS object — see `updateOrder` and `cancelOrder` in
`apps/web/src/components/customScripts/scriptAdapter.ts`, both of which look
the row up by id and throw when it is gone. Bybit's `updateOrder` cancels and
re-places from `{...order, ...update}`, so a forged `symbol` or `amount` does
not describe an order wrongly, it becomes a new live order on whatever was
named.

`UpdateOrderPatch.order` is optional for exactly this reason: every adapter
discards the caller's copy, so a caller holding only an id must not have to
fabricate one.

`OemsOrderAddress` and `OemsPositionAddress` are therefore the whole of what a
consumer may send back. Everything else in the context is for READING, and
widening an address is a security change, not a convenience.

`positionMode` rides every position for the same class of reason: in `hedged`
mode `(account, symbol)` addresses two positions, and a reduce that omits the
side is ambiguous rather than merely imprecise. `isSpot`, `isInverse` and
`contractSize` ride every market because a percentage of a position is a
different number on each.

## The resolver is the deterministic half

`resolveOemsPlan(plan, context)` is where an intent becomes the exact calls
that would be made. It is pure, so the confirm card the user reads is generated
by code that can be tested rather than by the model that wrote the sentence.

**Intents are higher-level than verbs on purpose.** "Reduce all my positions by
30%" is one thing the user said and N orders on a book whose sizes the planner
cannot see. Asking the model for the N orders means asking it to do lot-step
arithmetic on an inverse contract. It names the intent; this does the maths.

**Rejections are output, not exceptions.** A plan touching five positions where
one account is still connecting resolves the other four and reports the fifth.
Throwing would lose the four, and a partial resolution is precisely what the
confirm step exists to show.

**The `args` arrays are the SDK adapter's, positionally** — `AdapterContext`
in `packages/script-api-core/src/adapter/`. That is the target boundary, and
it is the same one custom scripts and the CLI hit. It re-checks everything
here; this resolver exists so a refusal is legible BEFORE the user confirms.

`cancelOrders` is sent an **id list**, never a symbol/side filter. The preview
says "Cancel 25 orders" and a filter would cancel every resting order matching
it — that gap between preview and execution is what `CancelFilter.ids` was
added to the SDK to close.

Consequences that look like bugs and are not:

- **A close is a reduce-only `placeOrder`, not `closePosition`, except on a
  plain linear market.** `placeOrder`'s `amount` is NATIVE — both the web and
  CLI adapters pass it straight to safe-cex — while `closePosition` defaults to
  base `contracts`. The gap is `contractSize`
  and it goes BOTH ways — over-sending above 1, where reduce-only clamps and
  hides it, and under-sending below 1, where an OKX `contractSize 0.01` market
  closes a hundredth of the position and reports success. So the size comes
  from `position.nativeContracts` and goes out as
  `placeOrder({ type: 'market', amount, reduceOnly: true }, account)`, which is what
  `closePosition` does internally minus the unit confusion. A FULL close on a
  plain linear market still uses `closePosition`, because there the units agree
  and naming the address lets the host read a size newer than the snapshot.
  "Plain linear" means `!isInverse && contractSize === 1`; spot is not excluded
  by that test and does not need to be, since a spot market carries no position
  to close. Spot is named where it does change an answer: the reduce-only
  refusal in the place path, and the leverage refusal.

- **On a plain linear market `close_positions` sends no size at all.** It names
  the address and the host reads the whole size off its own position, which is
  newer than the snapshot by the time anyone clicks. Elsewhere it has to send a
  number, for the unit reason above.
- **A reduce is an UPPER bound.** Where `metadata.autoDownSizeReduceOrders` is
  set, `downSizeReduceOrder` shrinks a reduce-only order further against resting
  ones, and the preview cannot know that.
- **Every `amount` is in the venue's own units** — what `market.amountStep`
  is quoted in, what `createMarketOrder` expects, and why the context carries
  `nativeContracts` / `nativeRemaining` alongside the base figures. The schema
  says so to the model; a plan that sizes off `contracts` on an OKX-style
  market is off by a hundredfold.
- **`minAmount` / `maxAmount` are checked only when `contractSize === 1`.**
  `precision.amount` is native everywhere; `limits.amount` is adapter-dependent
  and not consistently anything — OKX and BloFin build `min` as `minSz × ctVal`
  while leaving `max` native, KuCoin and Crypto.com report both
  natively despite setting a `contractSize`. No single conversion is right for
  all of them, and a wrong one refuses every order on a whole venue. So
  `limitsComparable` skips the check where the units may differ, and the venue
  does the refusing. Losing the nicer message is the right thing to lose.
- **The preview rounds to NEAREST everywhere.** `shouldRoundSizeDown` is
  venue-specific and lives in `apps/web`, so on those venues the preview can sit
  one lot step above what is placed.

The context carries `fatFingerNotional` for the same reason: the host ENFORCES
it by throwing from inside the order path, so without it here an over-size
order looks fine on the confirm card and then fails mid-batch. The exemptions
have to match the host's exactly — reduce-only and stop/take-profit are exempt
there, and refusing a close because the position is large would be the
protection working backwards.

**An opening order with no price reference is REFUSED, not waved through.** A
linear notional needs a price; one with no limit price and no `lastPrice` used
to compute as 0 and pass the limit. The command bar made it reachable, since it
can name a symbol whose ticker never loaded.

A stale context refuses the WHOLE plan rather than resolving against it. "Reduce
by 30%" against minute-old sizes is a wrong number wearing a confirm button.

## The plan schema is flat, not `anyOf`

`anyOf` is legal under OpenAI strict mode and would read better. It is not used
because strict mode has no notion of "required given `kind`" — each arm still
lists all of its own fields as required — so arms buy documentation rather than
validation, at the cost of the model having to pick a branch before it can fill
anything in. The per-kind requirements are enforced in `parseOemsPlan`, which
is also where `null` becomes an OMITTED key: the host's `updateOrder` decides
"price or amount?" with `Object.hasOwn`, so a key present and `undefined` counts
as sent.

## The prompt and the bounds live here too

`prompt.ts` carries the rules the JSON Schema cannot express — which fields a
`kind` needs, what unit an `amount` is in, what an empty scope means — because
those belong next to the types they describe and because the prompt version has
to move in lockstep with `OEMS_PLAN_SCHEMA_VERSION`; together they are the
`prompt_cache_key`, so a stale pair would serve the previous contract.

All FOUR untrusted inputs — the context JSON, the transcript, the user's
message and the results of reads run this turn — sit inside one per-request
nonce fence. A fenced context beside an unfenced transcript is the whole
attack, not half of it. Read results joined them when chained reads landed, and
they are why the nonce is re-rolled PER HOP rather than per turn: a result
echoes the model's own `args` back into the fenced payload, so a turn-long
marker could be closed from inside one. The stronger defence is structural:
this model cannot act. It returns a plan, `resolveOemsPlan` decides what the
plan means against live state, and a human confirms the result.

`boundContext.ts` caps what may reach that prompt. The caps are an ABUSE bound
— far above any real book — and truncating never THROWS: raising would turn
"you hold a lot of positions" into a failed request. It is not silent, though;
both a truncated READ RESULT and a truncated SNAPSHOT are marked for the model
(see above), and rule 5d names both markers from the cached instructions so
they are not lines the model has been told to disregard.
Every row is rebuilt FIELD BY NAME rather than spread, which is the same rule
the context types are built on and the reason a credential added upstream
cannot ride along.

**Which cap applies comes from the manifest's `resultShape`, via
`readShapes.ts`.** It used to come from `tool === 'getOrderbook'` and
`tool === 'getCandles'` against a parameter typed `string`, which made this
package the last holder of a hand-written read vocabulary and meant a rename
dropped the book cap in silence. `readShapes.ts` is its own file because
`boundContext.ts` and `prompt.ts` both need the map and the former already
imports `OemsReadResult` from the latter.

**An unknown tool is not an error.** `boundReadResults` runs in the route
before anything validates the name, so a forged body and a client one deploy
behind both reach it. Throwing would not even surface honestly: `readBody` sits
inside the route's `JSON.parse` try, so a raise from in here is answered as
"Request body must be JSON" — a 400 about the wrong thing. The value's own
shape decides instead, and a book is still capped at 20 levels a side under a
name nobody recognises. `isBook` proves only that the KEY is there, so both
sides are `Array.isArray`-guarded before slicing; `{bids: 5}` from a venue used
to throw.

**The truncation ADVICE is derived from params, not from the shape**
(`NARROWING_BY_METHOD`). The two do not line up: `getMarkets`, `getPositions`
and `getTickers` are all `rows`, but only the last two take a symbol, and a
model told "there is no way to page past this" will not go on to ask about a
symbol it can already see in the snapshot. `getOrderbook` is the opposite trap
— it takes a `depth` that looks narrowing, and the 20-level cap is applied
after the venue answers.

## Where the slice actually runs

The write half is served by the web and CLI hosts. It is **not** served on
mobile: `MobileCoreAdapter` throws on every write by design. The read half runs
everywhere. The full table, and the separate question of what a PUBLISHED
script may call, are in
[`packages/script-api-core/CLAUDE.md`](../script-api-core/CLAUDE.md) → "Which
parts of the SDK run where". Read it before assuming a host serves this.

`apps/web/src/components/agent/oemsSurface.test.ts` and
`apps/cli/src/script-adapter.test.ts` pin the two hosts that do serve it.

## Cross-doc

- Plan and ramp → `docs/plans/AI_OEMS_v1.md`.
- The shared SDK surface this slices, and the venn diagram →
  `packages/script-api-core/CLAUDE.md`.
- Why the writes must go through the OEMS layer rather than the raw exchange →
  `docs/custom-scripts/CONVERGENCE-AUDIT-v1.md` §F2.

## A user edit enters the RESOLVER, it does not patch the resolver's output

`ResolveOptions.overrides` replaces a derived size or price *before* resolution
continues, so lot-step rounding, `minAmount`/`maxAmount`, the
`Math.min(…, full)` clamp and `fatFingerNotional` all re-run against the user's
number exactly as they would against the model's.

**Patching `command.args` afterwards is the obvious shape and it is wrong
twice.** `args[0]` is an options bag only for `placeOrder` — `cancelOrders` and
`updateOrder` put an order ID there and `setLeverage` a symbol, so a blind patch
corrupts them. And every check above is private to `resolvePlan.ts`, so a patch
applied outside skips all of them. **Fat-finger is enforced nowhere else on this
path**: `prepareOrder` excludes it by design and the Agent never calls the web
`createLimitOrder` that has it, so a post-hoc edit would reach a venue with the
user's own limit unchecked.

Two consequences worth keeping:

- **Identity is the step index PLUS `symbol` + `account` + `side`.** Neither
  half is enough on its own. A bare index is wrong because one
  `reduce_positions` step produces a command per position. Symbol + side is
  wrong because one plan can hold a reduce and a stop on the same symbol,
  account and side — editing the reduce silently shrank the stop too and left
  the rest of the position unprotected. Side is still what keeps the two legs
  of a hedge apart: a long's close is a sell, a short's a buy.

  `OemsCommand.step` carries that index. It is stamped centrally in
  `resolveOemsPlan`, because a step resolver does not know its own position in
  the plan — hence `UnstampedCommand` for what the resolvers emit.
- **An edited `close_positions` leaves the `closePosition` fast path.** That
  path names the position and lets the host size it, which is better when the
  size is derived and impossible when it is typed — there is nowhere to put the
  number. An override drops it to the explicit sized order instead.

  **On a HEDGED position that edit is refused rather than performed.** The fast
  path is doing a second job there: `closePosition({ symbol, side })` names the
  leg, and `PlaceOrderOptions` has no field that can, so dropping to a bare
  reduce-only order could shrink the wrong side. Hedged positions on inverse or
  contract-sized markets already take the sized path and are unaffected — the
  refusal is exactly as narrow as the protection the fast path was providing.

`cancel_orders`, `amend_order` and `set_leverage` take no overrides on purpose:
a cancel has no size, and an amend's number is the user's request already rather
than a derived value to override.

## `OemsCommand.editable` is the confirm card's edit affordance

Present only where an override would be HONOURED, so a UI can render a pencil
from its presence alone rather than re-deriving the rule. It carries the
symbol, account, order side, amount and — only where the order has one — a
price.

It exists so a view never has to cast `args[0]` to find out what a row is:
that field is an options bag for `placeOrder`, an order id for a cancel and a
symbol for a leverage change, and a view reverse-engineering the resolver's
output is exactly the drift this package exists to stop. The resolver has these
values in hand when it builds each command.

Absent on: cancels, amends, leverage changes, and any close of a HEDGED
position — on both the linear fast path and the sized path, because a bare
reduce-only order cannot name a leg either way. Absent `price` on a close or a
market order, both of which the resolver refuses a price for.
