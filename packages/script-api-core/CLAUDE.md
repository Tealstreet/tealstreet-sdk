# CLAUDE.md — @tealstreet/script-api-core

Spec source of truth: [`docs/custom-scripts/SPEC.md`](../../docs/custom-scripts/SPEC.md). Update there, not here, until this package has gotchas worth documenting.

## Review notes

- **`api.exchange.runCommand` depth is a recursion guard, not a parallelism
  limiter.** The spec describes capping recursive `runCommand` dispatch
  (for example script -> runCommand -> script -> runCommand). Current
  implementation carries depth per dispatch context via
  `RunContext.runCommandDepth`; concurrent sibling calls from the same
  script all dispatch at depth 1. The internal proxy envelope carries
  `depth` to the host adapter, but the public script-facing
  `api.exchange.runCommand(command, opts)` options remain only
  `{ account?, symbol? }`.

## The order policy lives here, not in `apps/web`

`src/order/` holds the numbers every host must agree on: `rounding.ts` (the
step arithmetic), `orderPricing.ts` (the `Market`-shaped rules plus
`venueSupportsInfinityStops`), `shouldRoundSizeDown.ts` and
`downSizeReduceOrder.ts`.

They moved out of `apps/web` because a host-local policy is a policy only one
host obeys, and that had already happened: the CLI adapter applied none of it.
`downSizeReduceOrder` took a web `OpenOrder` (`Order & { account }`) and reads
none of the extra field, so it takes `readonly Order[]` here.

**`oems-core` consumes this package, never the reverse.** `rounding.ts` started
in `oems-core`; moving the policy here would have closed a cycle
(`script-api-core` -> `oems-core` -> `script-api-core`), which turbo fails the
whole build over. The arithmetic therefore sits with the policy, and
`oems-core` imports it for the preview resolver.

## Which parts of the SDK run where — the venn diagram

There are **two different questions** and conflating them has already caused
one wrong assumption. Read both rows before concluding anything runs anywhere.

### Axis 1 — what a HOST implements (`CoreAdapter` / `AdapterContext`)

| Host   | Bindings                                                        | Reads | Writes                              |
| ------ | --------------------------------------------------------------- | ----- | ----------------------------------- |
| web    | `apps/web/src/components/customScripts/scriptAdapter.ts`        | yes   | yes                                 |
| cli    | `apps/cli/src/script-adapter.ts`                                | yes   | yes                                 |
| mobile | `src/mobile/scripts/runtime/mobileCoreAdapter.ts` (mobile repo) | yes   | opt-in only — `allowWrites`, default off |

Those files are `HostBindings`, not adapters. The BODY is
`src/adapter/createCoreAdapter.ts` and every host runs the same one.

**Mobile's refusal is about SANDBOXED USER CODE, not about mobile.** Mobile
already trades: its order form, quick-trade and position rows call the synced
`@web/order/*` primitives directly, with no `CoreAdapter` in the path. What was
refused is this SDK's write slice — the surface custom scripts, custom modules
and an OEMS caller reach — and `createMobileHostBindings` takes `allowWrites`
(default false) to serve it. Nothing sets that yet: the two production callers
are the script and module bridges, which keep their own two locks plus the
WebView stubs. The caller it exists for is a first-party OEMS executor, and
that one must gate per METHOD (`isOemsWriteMethod`) rather than on the boolean.

So the OEMS read slice evaluates on all three hosts, and the write slice can
now evaluate on all three.

The CLI adapter USED to be thinner than web's in a way that mattered — its
`toSafeCexPlaceOrder` was a spread and a cast, applying no rounding and no
minimum-size validation, where web routed every size- and price-bearing write
through the order policy. Same contract, different numbers.

That is fixed: there is one `createCoreAdapter` body and all three hosts run
it, so the CLI now rounds, validates venue minimums, honours the infinity-stop
carve-out and applies the reduce-only clamp. A CLI script that used to send an
unrounded size below the lot step is now refused locally instead of by the
venue.

### `getExchange` is resolved per write, never captured

`createCoreAdapter` calls `requireExchange(account)` inside each write rather
than once per method, and `closePosition` re-resolves **per leg** because it
awaits once per position. On web, `host.getExchange` returns the live
`Exchange` from `exchangeAccountController`, which DISPOSES and REPLACES that
object on every reconnect — so a hedge-mode close holding one handle sends its
second leg to a dead exchange and the short side silently stays open. The
pre-flight `requireExchange(account)` at the top of `closePosition` exists only
to fail fast on a disconnected account; it is deliberately not the handle the
writes use. `createCoreAdapter.test.ts` pins this by handing back a fresh
`placeOrder` from every `getExchange` call and asserting the two legs used
different ones.

### The policy is `order/prepareOrder.ts`, NOT the adapter closure

`toSafeCexPlaceOrder` and `clampReduceOnly` used to live inside
`createCoreAdapter`, which meant reaching them required holding an adapter —
and therefore `HostBindings`, an account name, and an account-to-exchange
resolver.

**The CLI's own trading commands have none of those.** `packages/cli-core`'s
`buy`/`sell`/`close`/`stop`/`scale`/`swarm`/`bump`/`move` run from THREE hosts
— the REPL, `apps/cli/src/script-adapter.ts`, and
`packages/cli-core/src/worker-host/router.ts`, which is the web app's
in-browser CLI worker — and two of those build a `CommandContext` carrying a
bare exchange handle with `accountName: undefined`. So the commands could not
take an adapter, and wrote to the venue raw: no tick rounding on a limit
price, no lot rounding, no minimum-size refusal, an `amount: 0` infinity stop
sent to the twelve venues that do not read one, and no reduce-only clamp
anywhere. That is a much larger surface than the `script:` path this package
had already unified, and it is the one people actually trade through.

Lifting the policy out swaps an ACCOUNT-shaped dependency for a MARKET-shaped
one, which every caller already has. `createCoreAdapter` keeps two thin
wrappers that supply the host-shaped inputs; `packages/cli-core/src/order/prepareCliOrder.ts`
supplies them from an exchange handle. `cliOrdersGoThroughPolicy.test.ts` fails
the build on a CLI placement that opens with anything else.

**`venueAutoDownsizesReduceOrders` does not throw on an unknown venue and
`venueSupportsInfinityStops` does** — deliberately opposite defaults, because
failing closed points opposite ways for the two questions. Do not "make them
consistent"; the comment on both explains which way each has to fail. The
non-throwing one runs on every reduce-only write, so a venue missing from the
metadata map used to fail the write rather than skip an optimisation.

### Axis 2 — what a PUBLISHED script may call (`RUNTIME_SURFACES`)

This is a **policy** set, not a capability set, and it is the one that surprises
people: **no runtime surface carries any `api.exchange.*` write path.**
`getApiPathAvailability('api.exchange.placeOrder', ['web','mobile','cli'])`
returns `allowed: false, excludedBy: ['web','mobile','cli']`, and
`getAllowedApiPathsForTargets(['web','mobile','cli'])` resolves to reads and
utilities only. That is deliberate — see the test literally named *"rejects
writes for cross-target marketplace scripts"* in
`packages/script-authoring/src/validate/checkTargetSurface.test.ts`. A script
published to the marketplace runs on someone else's account, so
`checkTargetSurface` refuses a write regardless of target.

`CORE_WRITE_API_PATHS` exists and is real — the adapters implement it — but it
is only reachable through `CORE_SHARED_API_PATHS`, which no runtime surface
references. Do not "fix" that by adding writes to a surface; you would be
turning off the marketplace write refusal.

**Consequence for `OEMS_*_API_PATHS`:** they slice `CORE_*_API_PATHS`, so they
describe what the adapters SERVE (axis 1), not what a published script may call
(axis 2). The AI order path is a first-party caller with a human confirm step,
which is why it may use the write half at all.

### Two path sets, one set of rules

`RUNTIME_SURFACES` here and `MODULE_RUNTIME_SURFACES` in
`@tealstreet/module-api-runtime` are **separate path sets, on purpose**. This
one describes the SCRIPT api (`api.exchange.placeOrder`, `api.getPositions`);
that one describes the MODULE api (`api.orders.placeMarketOrder`,
`api.hooks.usePositions`, `api.antd`). Both are `api.${string}`, so merging
them would let a module path satisfy a script target check and the other way
round. Do not "unify the surfaces".

Everything AROUND the path set IS shared, because none of it is
module-specific: the target union, the runnable subset, the descriptor and
availability shapes, and `narrowToRunnableTargets`. Those were duplicated
byte-for-byte in both files — including the "an empty target list means all of
them" fallback, which is exactly the kind of rule that ends up silently
different in one copy. `moduleTargetSurface.ts` now imports them and keeps the
`Module*` names as aliases.

A third copy of the runnable list lived in
`apps/web/src/app/api/modules/marketplace/marketplaceTargets.ts` and is now the
same aliased constant. Its local membership SET stays local: it answers "is
this target runnable", where `narrowToRunnableTargets` answers "which targets
does this list mean". Swapping one for the other would turn a publish with no
runnable target from a refusal into a publish to every runtime.

### Axis 3 — where an order can ORIGINATE, and what enforces it

Axes 1 and 2 are about the SDK. This one is about the product, and it is the
one that was actually wrong: six separate places sent a price or a size that
was not what the user meant.

The set of files in the **v3 stack** that may call `exchange.placeOrder` is
closed and enumerated in `src/adapter/orderEntryPoints.test.ts`. A new one
fails the build.

`v2/` is excluded, explicitly and with the reason written in that file: it is
the vendored legacy frontend with its own chaser and TWAP instances placing
their own orders, and it consumes none of this SDK. Folding it in would mean
porting v2 onto the v3 order path — a product decision, not a guard. The claim
is about v3, and saying which is the point. Within that set, three narrower rules are enforced next to the code they
govern, because a guard in this repo cannot see the mobile repo and a guard in
one package should not reach into another:

| Rule                                                | Enforced by                                                            |
| --------------------------------------------------- | ---------------------------------------------------------------------- |
| one `CoreAdapter` body, hosts supply bindings       | `src/adapter/noHandRolledAdapters.test.ts`                             |
| a chaser starts once, on web                        | `apps/web/src/order/chaseOrder.guard.test.ts`                          |
| every CLI placement goes through the shared policy  | `packages/cli-core/src/order/cliOrdersGoThroughPolicy.test.ts`         |
| the same two rules for mobile's hand-rolled screens | `src/mobile/components/orderPathGuards.test.ts` (**mobile repo**)      |

**The web UI path and the headless path are two entry points on purpose, and
that is not the duplication to remove.** `apps/web/src/order/*` adds toasts,
`playOrderBeep`, optimistic order ids and the fat-finger modal; fat-finger
blocking a keystroke is right for a human at a button and wrong for a pipeline
with nobody to dismiss it. What must not be two is the NUMBER, and that is
enforced by both paths calling `order/prepareOrder.ts` —
`createStopLossOrder.test.ts` asserts its payload against `prepareOrder`'s own
output rather than a literal, so a change to the shared rule that one path does
not follow fails the build.

### The mobile sync

`tealstreet-mobile` copies `apps/web/src` into `src/web/`, copies every
`packages/*` directory except `tealchart`/`tealscript` into `src/packages/`,
and REWRITES `@tealstreet/x` specifiers — a bare barrel becomes
`@packages/x/src`, a subpath becomes `@packages/x/...`
(`tools/sync/src/transform.rs`). Both forms resolve, so a new package reaches
mobile automatically and neither import style is a hazard.

Two things that ARE worth knowing: mobile's `tsconfig.json` excludes
`src/web/order/**` and `tsconfig.mobile.json` excludes `src/web/**`, so synced
web files are not all in mobile's typecheck root set — a break there will not
surface as a mobile typecheck failure. And `src/package.json` is generated from
`apps/web/package.json`, so a dependency added there arrives on the next sync
without anyone editing the mobile manifest.

## `closePosition`'s fallback size is NATIVE

`opts.amount` is native by contract, and so is what `placeOrder` takes — but
the `?? position.contracts` fallback was BASE, carried over verbatim from the
old web adapter. They differ by `market.contractSize`, so a bare
`api.orders.closePosition(position)` asked for a thousandfold on OKX's
`DOGE-USDT-SWAP` and a fraction of the position on a Bybit inverse market. It
goes through `nativePositionSize` now, and `createCoreAdapter.test.ts` pins
both halves — the fallback converts, an explicit `amount` does not.

`resolveOemsPlan` only routed to this verb when `contractSize === 1 && !isInverse`,
which is the shape of a workaround for a bug nobody had written down.
