# CLAUDE.md — apps/web Cypress E2E suite

End-to-end tests for the Tealstreet trading platform — exercises real exchange connections, order placement, position management, and audit flows.

- **Layout**: `e2e/`, `support/`, `fixtures/`, `config/`
- **Exchange credentials**: `cypress.env.json` and `cypress/config/exchanges.js` (local-only, never committed)
- **`apps/web/.gitignore` ignores `cypress.env.json*`, not just the exact name.** A `cypress.env.json.bak` was committed on 2026-08-09 and untracked later; history was deliberately not rewritten, so it remains in old commits. Keep backups and variants of the credentials file out of git — the widened pattern exists for that.
- **What `all` means**: `config/exchangeLists.js` derives `VALID_EXCHANGES` from every non-`_` key of `exchanges.js`, and `all` is exactly that configured list. It is the **single definition of the `all` matrix**, consumed by `run-e2e.js`, `run-e2e-parallel.js`, and — via `scripts/list-exchanges.js` — by `the e2e workflow (mirrors/tealstreet-sdk/.github/workflows/e2e.yml)`'s setup job. CI used to hardcode its own copy of the list and drifted; don't reintroduce a second allowlist anywhere. The `workflow_dispatch` `exchanges` input is deliberately a string (`all`, one exchange, or comma-separated exchanges), not a choice list, so GitHub UI options cannot drift from the config.
- **`allow_foreign_flatten` is a deliberate maintenance lever, not a CI setting.** `assertNoForeignPositions()` normally refuses to run when an account holds a position outside its configured test symbol — it was added after the suite silently liquidated an unrelated +$18 position. The `allow_foreign_flatten` **workflow_dispatch** input flips it to "log loudly and close anyway", which is how leftover dust gets cleared through the real app rather than hand-rolled REST (that bypasses the proxy/tunnel routing IP-whitelisted keys require and returns `50110 invalid IP`). The `schedule` trigger renders `inputs.*` as `''`, so the nightly always keeps the hard guard — and the spec compares `String(Cypress.env('allowForeignFlatten')) === 'true'` because Actions renders an unset or false boolean as `''`/`'false'`, **both truthy as JS strings**. Never set it for a routine run.
- **Adding an exchange config makes it part of `all` and is destructive.** `cleanup()` → `flattenPositions()` is symbol-agnostic: it closes every position on every registered account before each test. Confirm an account is dedicated to testing first — the well-funded keys on the sheet are well-funded because someone is using them.
- **`skipTests` is not a place to park failures.** Every entry was cleared in favour of real data, because the accumulated list was masking genuine defects rather than recording venue limitations (bybit's two skips hid a stale `fetchPositionMode`; blofin's hid a 400 from `set-position-mode`). Before adding one back, prove the test _cannot_ pass — a venue capability that's checkable in `exchangeMetadata` (e.g. `positionModes: [OneWay]`) belongs as a metadata-driven gate in the spec, not a per-exchange skip list. Record the evidence in the comment.
- **Run**: `yarn test:e2e` (serial), `yarn test:e2e:parallel`, `yarn test:e2e:list`
- **First run in a fresh checkout/worktree needs `yarn test:e2e:vendor`.** `spec.cy.ts` imports `../support/vendor`, a generated esbuild bundle of the workspace packages (gitignored) that lets Cypress run without the whole monorepo built. Without it the run dies in webpack with `Can't resolve '../support/vendor'` before a single test — which reads as a broken spec rather than a missing build step. CI runs the equivalent as its own step; the yarn script exists so local runs don't have to invoke the file by hand. Re-run it when `@tealstreet/math`, `safe-cex` or `proxy-ips-lists` change.
- **`cypress/tsconfig.json` exists for the public mirror.** Cypress's ts-loader needs a tsconfig at the project root or in `cypress/`, and the tealstreet-sdk mirror ships `cypress/` without `apps/web/tsconfig.json`, so every job died with `TsConfigNotFoundError` before a test ran. Workspace imports reach the spec through the vendor bundle, so it carries compiler options only.
- **`cypress.env.json` is not created by `wt-setup`.** Symlink it from the main checkout rather than copying, so the credentials exist in one place only.
- **A fresh remote machine must match the working local env, not just the Cypress env.** For a Mac mini / spare-host loop, copy the repo root `.env`, `apps/web/.env.local`, and `apps/web/cypress.env.json`, then run `yarn install` and `yarn workspace tealstreet-next test:e2e:vendor`. If the test fails in `beforeEach` with `GET /api/custom-modules/drafts?meta=1 -> 500` and the dev-server log says `SUPABASE_SERVICE_ROLE_KEY or SUPABASE_SERVICE_SECRET is required`, the app server is missing its service-role env; set `SUPABASE_SERVICE_SECRET` or `SUPABASE_SERVICE_ROLE_KEY` in `apps/web/.env.local` and restart `next dev`. Do not "fix" this by ignoring arbitrary 500s in Cypress — the invariant is that the remote host should behave like Sam's local dev server.
- **Leave remote dev servers detached unless asked to kill them.** Start the remote host's `next dev` with output redirected to a log file so it survives the SSH session; Cypress should run on that same host against `http://localhost:3000`. Environment changes still require one explicit server restart, because Next reads `.env.local` at process start.
- **Note**: tests hit live testnets where available; treat any test that touches mainnet as needing extra review
- **Local fast iteration and CI both use exchange-server mode, but not the same endpoint.** The Mac mini loop intentionally keeps `apps/web/.env.local` pointed at `NEXT_PUBLIC_EXCHANGE_SERVER_URL=ws://localhost:23456` and runs `yarn workspace @tealstreet/exchange-server dev` locally. GitHub e2e sets `CYPRESS_useExchangeServer=true` and relies on the deployed app's hosted default, `wss://connect.tealstreet.io/account`; do not change the mini env to hosted just to mirror CI unless explicitly validating the hosted path.
- **Failures**: Cypress writes screenshots and videos to `cypress/screenshots/` and `cypress/videos/` (gitignored)

## Order-form invariant: price before size

**Always call `setBuyPrice`/`setSellPrice` before `setOrderSize` on any
price-bearing tab** (limit, TP/SL, brackets). The size cell is
quote-denominated but the order form stores base, and
`useSizeOption.onChangeQuote` returns early when `price <= 0`. The limit
tab mounts at price `0`, so a size written first lands **only** in the
input's internal display state — `setSize` is never called — and the
sync effect wipes it back to `0.00` on the re-render the price write
triggers. The order then submits with size 0 and the app throws
`trade.cannot-place-order-size-too-small`, which Cypress surfaces as an
`UNCAUGHT ValidationError` and, downstream, as `expected 0 to equal 1`
on the open-orders table.

This is silent in both directions: the input _displays_ the size you
wrote, and no request is ever made, so the API log shows no order POST
and no exchange error. Market tabs are immune because they price off
the live ticker and never pass through 0. If a limit-family test
regresses to "no order placed", check this ordering first.

## Cypress queue ordering

Commands queued inside a `.then()` callback can race with commands queued after
the callback site. If cleanup depends on callback work finishing first, queue
the cleanup inside the same callback after the actions/assertions it protects.

Select readbacks after worker-backed mutations must be retryable. Position mode,
margin type, and leverage changes can round-trip through the worker, exchange
REST, and store reconciliation after the click resolves; a one-shot `.then()` can
see the old select value even when the exchange request has already succeeded.

## `crossDelta` vs `farTouchDelta` — venues cap crossing limit prices

`farTouchDelta` places orders that must **rest**; `crossDelta` places orders
that must **cross and fill**. They were one knob until WOO exposed the
difference: `PERP_ETH_USDT` publishes `bid_cap_ratio: 1.01` /
`ask_floor_ratio: 0.99`, so a buy priced 1.5% above the touch is rejected
outright with `The order price does not meet the price range requirement of
this instrument` — the position never opens and the reduce/hedge tests fail
on a _positions_ assertion with no obvious link to price.

`crossDelta` defaults to `farTouchDelta`, so only venues with a published cap
need to set it (woo: `0.008`). **Don't collapse them back into one value** —
lowering `farTouchDelta` to satisfy the cap would move resting orders close
enough to the touch to fill by accident, breaking the limit tests that
currently pass. Check the venue's own market metadata before assuming a
crossing price is legal; the cap is per-symbol.

## Woo balance and destructive order sizing

The shared WOO account was refit to the roughly `$20.5` total / `$15` free
collateral tier on August 12, 2026. Keep WOO's destructive E2E order notional
small (`orderSizeInQuote: 10`) unless the account is deliberately topped up and
the balance expectation is refreshed in the same change. WOO has a per-exchange
`balanceVariance` override so stale funding assumptions fail the `Load Balance`
test instead of letting later position/reduce tests fail opaquely.

WOO also has separate open-position and cleanup wait behavior. Keep
`openPositionSettleWaitTime` short enough to assert the just-filled position
before this shared account can flatten again, and keep
`slowUpdateExtraWaitTime` for close/cancel convergence where waiting for REST
polling is still useful.
During position-heavy Cypress runs WOO can return `/v3/positions` 429s with a
"try again in 10 seconds" message. Keep WOO's E2E `accountExtra.tickInterval`,
`actionWaitTime`, and `slowUpdateExtraWaitTime` aligned to that 10s cadence
rather than skipping position coverage.

WOO's inspected reduce-only limit assertion also uses `reduceOnlyLimitPostOnly`.
The generic reduce-order test wants to inspect a resting reduce order before it
is amended/flattened; without post-only, WOO can fill the reduce limit
immediately and the open-orders assertion correctly sees zero rows. The final
crossing reduce close in that same test must force post-only off, otherwise WOO
rests the close order and the short remains open.

WOO market and limit setup in the generic spec uses the mounted
`exchangeAccountController` to call the real browser-side `placeOrder()` after
the UI size/button state has been verified. The raw click path has produced
plain `LIMIT` orders where the test intended `POST_ONLY`, and market-IOC orders
that cancel flat before a position opens. The controller path still exercises
the actual WOO adapter (`OrderType.Market` -> IOC, `PostOnly` -> `POST_ONLY`)
while removing the UI event race from exchange-API coverage.
When the WOO limit helper is testing a bracket order, pass the TP/SL prices
through to that same `placeOrder()` call. Otherwise the helper creates only the
entry limit and the all-orders assertion correctly sees one order instead of the
entry plus two positional TP/SL children.
Pass the quote size being tested into that direct market helper. Several tests
intentionally override the generic `orderSizeInQuote` (`marketOrderSizeInQuote`,
position flip, partial-close open), and recomputing WOO's base amount from the
global default under-tests the UI state that was just asserted.
For the crossing post-only negative check, the WOO direct limit helper accepts
the adapter's `post-only ... would cross ...` preflight rejection as the expected
canceled outcome. WOO has filled marketable or near-fair `POST_ONLY` buys instead
of canceling them, so the real safety behavior lives in safe-cex preflight.
Keep WOO on a symbol whose current public book is inside the venue `price_range`
on both sides. As of 2026-08-16, BTCUSDT satisfies that; ETHUSDT can publish an
ask just outside the buy cap, which makes market-buy IOC emulation impossible to
fill without WOO rejecting the price. Also normalize WOO setup leverage before
market-position tests; the shared BTC account was left at 98x and tiny e2e
positions flattened within seconds, while 10x kept the same real fills stable.
Hosted exchange-server CI forces WOO's Cypress account through Tealstreet REST
proxy routing when targeting preview/v3/dev. The GitHub `CYPRESS_ENV_JSON` secret
still carries the direct local account shape, which is correct for mini
`localhost:3000` runs; hosted connect private reads otherwise egress from the
container IP and WOO returns 403.

Phemex position-mode tests must use the worker `changePositionMode` path and
verify with `resolveLeverageContext`. The visible selector can be absent or stale
while the venue state changes correctly per symbol, so do not use selector
readback as the source of truth for Phemex mode changes.

Size WOO and Binance reduce closes in **base** from the displayed position, not
with the generic quote-size overshoot. WOO rejects oversized reduce-only orders
with `Insufficient position for reduce only order`; Binance accepts and clamps
them exchange-side, but the open-orders table can still show the pre-clamp input
in `Remaining`.

Keep WOO out of the reduce-order amend sub-check. Plain resting limit amendment
is still covered by the limit-order test, but WOO reduce-only orders can vanish
between readback and amend when the underlying short-lived test position has
already flattened.

## Cleanup is destructive — the suite flattens EVERY position on the account

`flattenPositions` → `closeAllPositionsViaController()` is **symbol-agnostic**: it
closes every position on every registered account and asserts zero remain. It runs
in `cleanup()` before each test, so **adding an exchange to the matrix is a
destructive act on whatever that account happens to hold.**

This is not hypothetical. Wiring up Kraken and running a single `load-balance`
test closed two unrelated positions eleven seconds apart — `PF_HYPEUSD` sitting on
**+$18** and `PF_XBTUSD` on +$2.61. They happened to close at a profit; nothing
about the design guaranteed that.

`assertNoForeignPositions()` now runs first and **throws** if any open position is
in a symbol other than the configured `symbol`, naming each one. A dirty account
fails the run loudly instead of getting liquidated quietly. Clear such positions
deliberately — through a legitimate runtime (safe-cex / the CLI / the web app), never
hand-rolled REST, which bypasses the proxy/tunnel routing that IP-whitelisted keys
depend on.

**Before adding any exchange to the matrix, confirm its account is dedicated to
testing.** A well-funded key on the shared key sheet usually means somebody is
using it — that's _why_ it has money in it.

## `symbol` vs `displaySymbol` — venues trade one string and display another

Two different identifiers are needed and one config value cannot serve both:

| surface                   | matches on                          | needs        |
| ------------------------- | ----------------------------------- | ------------ |
| market dropdown           | `data-symbol` = raw `market.symbol` | `PF_DOGEUSD` |
| `last-trade-title-<sym>`  | testid off the app's `activeSymbol` | `PF_DOGEUSD` |
| orders / positions tables | cell **text**                       | `DOGEUSD`    |

`useOpenOrdersColumns` renders `{market?.prettySymbol || symbol}`, and Kraken's
`prettySymbol` strips the `PF_` prefix — so `'DOGEUSD'.includes('PF_DOGEUSD')` is
false and `findOrder`/`findPosition` return nothing. The failures read
`Could not find any orders for PF_DOGEUSD` and `expected undefined to not equal
undefined`, neither of which points at symbol formatting; meanwhile the orders are
resting on the exchange perfectly happily.

`displaySymbol` defaults to `symbol`, so only venues where the two differ set it
(kraken: `'DOGEUSD'`). Pass it to `findOrder`/`findPosition`; keep `symbol` for the
dropdown and the ticker testid. Any venue whose `prettySymbol` is not identical to
`symbol` — prefixed, slashed, or dashed — needs this.

## `quoteCurrency` — the size cell's unit is not always USDT

`ensureQuoteSizeUnit` flips the unit-switchable size cell from base to quote and
then **blocks on the cell's text** before writing, because
`setNativeInputValue` targets a captured node and a write made before React
swaps it is silently lost (see the price-before-size invariant above). That
assertion needs the venue's actual quote label.

It used to be the literal `'USDT'`, which is wrong for any USD-quoted venue:
Kraken Futures perps are `PF_DOGEUSD` and the cell reads `SizeUSD`, so every
order test failed with `expected '<label…>' to contain text 'USDT', but the
text was 'SizeUSD'` — five tests, one cause, and the message points at the
input rather than the config.

`quoteCurrency` defaults to `'USDT'`, so only non-USDT venues set it
(kraken: `'USD'`). Coin-margined and USDC variants will need it too. Note this
is the **displayed unit**, not the settlement currency — read it off the cell,
don't infer it from the symbol.

Kraken Futures also does not expose the generic leverage slider in this E2E path
(`canSetLeverageInE2E()` is false). The position-mode test still opens a real
position, checks the position row, and flattens it; only the slider read/set
assertions are gated off. The shared Kraken E2E account is flex-funded, so its
config narrows `marginTypes` to `['cross']`: isolated `PF_DOGEUSD` orders draw
from the zero-collateral `fi_dogeusd` account and reject as `wouldCauseLiquidation`.
Keep `positionModeOrderSizeInQuote` at the minimum one-contract size; larger
market opens can trip Kraken's liquidation-risk precheck even on cross.

## Paradex capability gates in the generic spec

Paradex is a first-class target, but several generic assumptions do not hold:

- The add-account form has no `apiSecret` field (`requiresNoSecret`) and instead
  requires `apiKey`, `subkeyAddress`, and `paradexAddress`
  (`requiresParadexAddress`). Fill credentials from `exchangeMetadata`; do not
  special-case a fake secret into the UI.
- Auth-failure tests must use the configured `symbol`, not a hardcoded Binance
  symbol like `BTCUSDT`. Paradex symbols are shaped like `DOGE-USD-PERP`, and a
  missing symbol can mask the credential failure path. For Paradex bad-key
  cases, corrupt the signing key while leaving `paradexAddress` and
  `subkeyAddress` structurally valid; invalid addresses can fail before the
  `/auth` request we are trying to exercise. The localStorage bad-account
  activation path has also been observed to bootstrap only public markets for
  Paradex, so its bad-key assertion uses the add-account validation path instead.
  Bad Paradex credentials can surface useful auth detail either in the request
  log or as captured failed HTTP from `/auth` (for example `invalid signature`)
  before the hosted preview app emits a Toastify error, so count either signal as
  the auth failure evidence.
- Generic leverage-setting assertions are disabled for Paradex in Cypress. The
  venue exposes leverage magnets, but the current e2e path observed the account
  remain at its existing leverage after the app attempted to set another value.
- Paradex market-order setup in the generic spec uses the mounted
  `exchangeAccountController` to call the real browser-side `placeOrder()` after
  the UI size/button state has been verified. Hosted preview can render an
  enabled market button and then produce no visible write trace from the raw
  click path, which turns exchange-API coverage into a UI event race. Keep this
  Paradex-only; other venues still click the market buttons.
- Paradex limit/bracket setup follows the same controller path. Hosted preview's
  open-order worker can temporarily overwrite recovered REST orders with an
  empty store before the table renders, so the positive limit-order assertions
  recover `fetchOrders()` into the store and assert the recovered store count
  directly. Bracket orders assert the three returned exchange order IDs; the
  stop-order test remains the detailed TP/SL readback coverage.
- Paradex rejects oversized reduce-only closes instead of clamping them to the
  open position. Keep the 2x close-size stress for venues that tolerate it, but
  use exact position-size closes for Paradex. On limit reduce closes, read the
  actual displayed position base size and write the base-size input; quote sizing
  can either overshoot and get rejected or undershoot and leave residual dust as
  the limit price moves.
- Paradex native amendment only supports exactly reconciled open LIMIT or
  untriggered TP/SL orders. The row-driven generic amend assertion can hit
  `unknown unknown` when the UI row is not exact enough, so generic amend
  validation is gated off until the test can identify the amendable order type
  precisely.
- Paradex open-order rows can display TP/SL type either as `Take Profit` /
  `Stop Loss` or with the trigger-label suffix (`[Mark]` / `[Last]`) depending
  on the readback path. Accept both forms and keep validating the trigger
  behavior.
- Paradex open-order rows can display an empty TIF for resting `GTC` / `Post`
  orders. Treat blank as equivalent to `GTC` and skip post-only TIF text
  assertions for that venue.
- Cleanup cancels open orders before closing positions, then checks orders again
  after positions are flat. Stale Paradex TP/SL and stop orders can otherwise
  block deterministic flattening or leave untriggered reduce orders behind.
- Paradex e2e uses `ETH-USD-PERP` for the generic matrix. Thin DOGE books have
  produced best bids below Paradex's legal sell-side price band, where a
  reduce-only IOC close is accepted then canceled unfilled.

## Add-account form selectors

The new API flow intentionally uses stable `data-testid` selectors for Cypress
instead of Ant Design's generated `#add-account_*` input ids. The account-name
field is nested under a labelled wrapper with `noStyle`, and the generated ids
are not a durable contract for the real input. Keep add-account E2E fills on the
`add-account-*-input` test ids exposed by `AccountForm.tsx` and
`ApiKeyFormFragment.tsx`. The exchange picker selects a tile first and advances
with the `Next` button; do not assume clicking `exchange-select-*` opens the
credential form by itself. After clicking `Done`, assert that the account was
persisted with a retrying check; credential validation can keep the button busy
for several seconds before persistence.
The Tealstreet proxy checkbox is a conditional transport control. If a Cypress
account has `tealstreetProxy` but the current add-account UI does not render the
checkbox, continue with the form defaults and let credential validation prove the
route; do not fail before the submit path is exercised. When it does render, the
Checkbox component puts `data-testid` on its actual `input[type=checkbox]`,
which is hidden by design; select that input directly and force-click it.
Auth-focused tests (`bad-api-key`, `add-api-key`) deliberately skip the generic
last-trade price bootstrap in `beforeEach`. They validate account setup and auth
failure evidence, not order pricing, and Paradex can load enough UI for those
flows before the selected market title is available.
Bitunix bad-key evidence may say `Token invalid` in request logs while the toast
says only `double check your key and secret`; count that as meaningful auth text
instead of weakening the strict toast requirement.
Bitunix bad-key runs can make just over 100 HTTP reads while private WS auth is
failing with `Token invalid`; keep the per-exchange request cap bounded at 120,
not unbounded.
Filtering the input itself with `:visible`, or requiring its wrapper to be
visible, silently skips the route switch and IP-whitelisted keys validate from
the runner's direct IP.

## Bitunix notes

- Bitunix standalone TP/SL orders require a partial amount strictly below the
  current position size. The generic stop-order test opens a position with
  `orderSizeInQuote`, so using that same quote size for the stop is effectively
  full-size and the exchange rejects it with `TP/SL amount must be less than the
size of the position`. Keep Bitunix on a smaller partial stop size and do not
  include it in the full-position stop variant unless the adapter gains a
  venue-native full-close path for this endpoint.

## OKX notes

- OKX bracket orders create TP/SL as algo orders. A successful cleanup cancel
  can be followed by `51400` "already filled, canceled or does not exist" on
  retry while `orders-algo-pending` is already empty; the exchange is flat, but
  the app store can still hold stale open TP/SL rows. Keep OKX on the hosted
  order recovery path and refresh after cleanup cancels before asserting zero
  open orders.
- The same hosted order recovery is needed before positive open-order table
  counts. OKX can accept the entry limit and both conditional TP/SL algos while
  the private store still shows only the limit leg; `fetchOrders()` is the
  deterministic readback for the bracket assertion.
- OKX can also retain stale TP/SL rows in the stop-orders table after a prior
  failed bracket run, even when the exchange has canceled those algos. The OKX
  bracket assertion therefore treats the all-orders table count as a lower
  bound for the visible limit leg and validates TP/SL through the safe-cex
  `fetchAlgoOrders('trigger'/'conditional')` readback by expected side and
  trigger price, instead of validating stale DOM rows.
- Validate OKX standalone stop orders through the same direct
  `fetchAlgoOrders('trigger'/'conditional')` path. OKX conditional algos can be
  accepted by the exchange while the stop-orders table/store stays empty, and
  cancel/replace can briefly surface an amended row with blank price text or
  `actualSz: "0"`.

## Phemex notes

- Phemex reduce-only limit orders can be placed and read back, but the generic
  table-driven amend path is not stable for reduce orders: the app cancels and
  recreates the reduce-only order, and live runs have observed only canceled
  reduce-limit rows left in the store while the position remains open. Keep the
  initial reduce-order readback coverage, but skip exact reduce-order amend
  validation there.
- Phemex full-position TP/SL submits `closeOnTrigger=true`, and repeatedly
  replacing those full-close stops across trigger types can return
  `TE_POS_ZERO_CANNOT_CREATE_TP_SL_ORDER` even while REST has just reported the
  position. Cover the full-position path once and use partial stops for broader
  trigger-type coverage.
- Phemex limit-order cleanup can leave amended GTC rows visible long enough to
  satisfy the first post-only count if the post-only phase reuses the same
  prices. Assert zero open orders after cleanup and use distinct farther
  post-only prices before validating the post-only pair.
- Phemex accepts and reads back resting limit orders, but exact amend sub-checks
  are not stable: amend is cancel+replace and the replacement can lag the
  open-orders table/store, leaving only the other side visible at the count
  assertion. Keep placement/readback coverage and skip Phemex resting-order
  amend validation.

## Order-button selectors

Classic order forms and the bybit-skin shell can coexist in the DOM. Use helpers
that filter to visible buttons (`clickMarketButton`, visible TP/SL button
selection) instead of raw `cy.get('[data-testid=market-*-button]').click()` or a
TPSL button click; otherwise Cypress can receive two matching subjects.
Order-type tabs have the same issue in tests that can exercise the bybit-skin
shell; use `setOrderTypeTab()` instead of direct `toggle-tabs-*` clicks in
those paths. That helper retries against either classic `toggle-tabs-*` or
bybit-skin `dropdown-tabs-*` surfaces because cleanup/mode changes can briefly
remount the form after the exchange load gate has already passed. Bybit-skin
TP/SL is selected through the `conditional_group` dropdown button.
Position-mode and margin-type selectors can also have hidden duplicates during
shell remounts. Read and click the visible selector instance only; Phemex has
proven the exchange-side margin switch can succeed while a stale hidden selector
still reads `Cross`.

## Slider helpers

`PercentSlider` commits changes on pointer release only if its pointer-down path
successfully captures a pointer. Cypress helpers that drive range inputs must
include a stable `pointerId`/`pointerType` payload; setting `.val()` and firing a
bare `input` event updates the DOM thumb but never calls React `onChange`.

## Kraken notes

- Kraken open-order readback returns post-only resting limit orders as plain
  `GTC` rows and does not render a leverage slider for the DOGE perp form. Skip
  post-only TIF text assertions there, do not run E2E leverage-setting paths,
  and derive margin-slider leverage from the 100% quote-size output when no
  leverage control is visible.
- Kraken market orders use a smaller `marketOrderSizeInQuote` than the rest of
  the suite and set `marketOrderSides: ['buy']`. `$2` limit/crossing-limit paths
  pass, including short-side coverage, but Kraken rejected the market-order
  short-open path with `wouldCauseLiquidation` even after the market test was
  reduced to one native DOGE contract. Keep the long market path for live
  market-order coverage without weakening the order tests that need larger
  resting orders.
- Kraken reduce-only close limits intentionally submit an oversized quote value
  so the exchange clamps the order to the current position. The API/store can
  show the clamped `amount` while the table briefly displays the pre-clamp input
  value in `Remaining`; validate the store amount for that numeric assertion.
- Kraken stop-market orders place and render, but native amendment is gated off: safe-cex rejects update when trigger-signal provenance is missing, and the table Price cell can be non-numeric after that rejected edit. Keep initial stop placement coverage and skip amend validation until trigger provenance is preserved end to end.

## Binance notes

- Binance is excluded from E2E leverage-setting because mode/leverage writes can
  return intermittent signed-request failures while trading paths still work.
  The margin-slider test must infer Binance's active leverage from the 100%
  slider output instead of trusting a visible max/leverage label; the form can
  size orders from the exchange-side leverage while the label reads a stale
  lower value. Keep those quote-size reads as retrying assertions: the slider
  thumb can update before the dependent size input recalculates, and a one-shot
  read can sample `0` even though the UI is still converging.

## Hyperliquid notes

- Hyperliquid's production add-account form gates manual credentials on
  builder-fee approval for the main wallet address. Cypress has no wallet signer,
  and the local `skip-builder-fee` bypass is development-only, so skip only the
  `add-api-key` UI submit flow there. The existing-account auth/trading tests
  still exercise the live Hyperliquid connection.
- Hyperliquid can leave a private-store echo for an already-gone reduce-only
  `stop_market` order with `amount: 0`; subsequent cancels return "Order was
  never placed, already canceled, or filled" while positions are flat. Treat
  only that zero-size stop residue as ignorable during cleanup convergence; do
  not filter real non-zero open orders.
- Hyperliquid partial stop rows can render a non-parseable `Remaining` cell even
  while the private store has the live open stop with the resolved base `amount`.
  Keep validating the table price/type/reduce fields, but fall back to the store
  amount for that one numeric size assertion when `Remaining` parses as `NaN`.

## BloFin notes

- BloFin's `set-position-mode` endpoint returns a body-level `code:"400"` for
  this account while `GET /account/position-mode` remains `long_short_mode`
  (Hedged). Keep the position-mode UI switch assertion skipped for BloFin; other
  order-path tests still cover the account in its live mode.

## Bitget notes

- Bitget is funded at a low-balance tier, but the venue rejects orders below
  `$5` notional. Keep `orderSizeInQuote` just above that minimum so precision
  and price drift do not submit under `$5`, while still leaving enough margin
  for hedge-mode tests to carry both legs at once; `$10` can pass single-order
  paths but fail the second hedge leg with `The order amount exceeds the balance`.
- Keep Bitget's balance variance tight enough to reject a dust balance. A `$0`
  account can otherwise pass `Load Balance` against the low `$6` expectation and
  fail later with no order POST because the form never enables a `$5.50` order.
- The hedge-mode test intentionally raises Bitget leverage via
  `hedgeModeLeverage`; do not lower the order size below the exchange minimum to
  work around balance pressure.

## `trigger-type-selector` lives behind the TP/SL "Advanced" disclosure

The TP/SL tab's trigger-type pickers are `LabeledDropdown`s (an
`InlineCellDropdown` under the hood), **not** the compact antd `Select`
that `TriggerTypeSwitcher` renders elsewhere — so `setSelectValue` does
not drive them. They also sit inside a `Disclosure` that is collapsed by
default (`storageKey: 'placeOrder:tpsl:advancedOpen'`), so they are not
in the DOM until the "Advanced" header is clicked. `setTriggerTypeOnBothSides`
handles both facts; don't revert it to a `Select` interaction.

The `data-testid="trigger-type-selector"` on those two dropdowns exists
**only** for this suite — same category as `ColoredNumber`'s `data-value`.
When a picker is rewritten into a different control, carry the testid over
in the same change, or the suite fails with a bare "expected to find
element" that looks like a timing flake.
Drive the TP and SL pickers by their row labels, not by index: both controls
portal menu items with the same option test ids, and a controlled
`InlineCellDropdown` now closes explicitly on selection so a stale menu cannot
steal the next Cypress click.

## Result JSON shape (after:run hook in `cypress.config.ts`)

The `cypress/results/<exchange>.json` files emitted in CI carry per-test rows
with three possible `state` values, **not** the two Cypress reports natively:

- `'passed'` — Cypress reported pass.
- `'failed'` — Cypress reported failure **and** the test actually ran
  (`duration > 0`).
- `'blocked'` — added by the after:run hook. Applies to
  `state === 'failed' || state === 'skipped'` with `duration === 0`.
  Either `beforeEach`/`before` threw (no prior test had executed) or
  a real failure aborted the spec and Cypress flagged every remaining
  test as `failed`/`skipped` without running them (Cypress mixes both
  states for the cascade depending on version + retry config — both
  collapse to `blocked`). The hook attaches a `blockedReason` field —
  `'beforeEach/before failed'` or `'spec aborted after prior failure'`
  — to disambiguate. **Don't treat these as real regressions** in
  dashboards/reports; the Slack reporter at
  `cypress/scripts/slack-e2e-report.js` renders them as `:black_circle:`
  / `[BLOCKED]` instead of `:red_circle:` / `[FAIL]`. `totalFailed` still
  counts blocked tests so the topline stays red, but a separate
  `totalBlocked` is exposed alongside it.

Per-test rows also carry an optional `error?: string` — first line of
Cypress's `test.displayError`, truncated to 500 chars. Populated for
real `state === 'failed'` rows so the Slack report can show _which_
assertion blew up under the test name (italic sub-line in the summary,
indented sub-line in the detailed report).

If you change the after:run shape, update this section in the same commit
and the Slack reporter together — they're coupled by the same JSON
contract.

### Diagnostic fields (Wave B)

- **`loadFailureDiagnostics.phase`** — `'env' | 'auth' | 'visit' | 'load'`.
  Tags which gate of `beforeEach` tripped. `'env'` covers
  `getAccount()`/`getExchangeConfig()` throws (sync). `'auth'` covers Auth0
  sign-in flow timeouts. `'visit'` covers the second `cy.visit(getHost())`.
  `'load'` covers `waitForExchangeLoad()`. The plumbing is a module-level
  cursor (`currentBeforeEachPhase`) read by a single `Cypress.on('fail')`
  handler registered at the top of every `beforeEach` and torn down before
  the test body runs (and again in `afterEach` as a safety net). The
  `'load'` path emits its own rich diagnostics from
  `captureAndThrowDiagnostics()` and sets `loadFailureLogged = true` to
  prevent the generic fail handler from double-emitting. **Don't add a
  second `Cypress.on('fail')` registration** — it'll run alongside the
  existing one and double-emit. Always pass the captured handler reference
  to `Cypress.off('fail', handler)` (not just `Cypress.off('fail')`)
  otherwise stale closures from previous tests stack up.
- **`loadFailureDiagnostics.error`** — string error message captured by
  the generic fail handler for env/auth/visit failures and any non-load
  cy.\* throw inside beforeEach.
- **`transportFailureReason`** (top-level on the result JSON, also nested
  on `loadFailureDiagnostics`) — one of `'tradetunnel-down' |
'wsohttp-timeout' | 'proxy-5xx' | 'proxy-rate-limit' |
'proxy-forbidden'`. Populated by `classifyTransportFailure()` against
  fetch/XHR response bodies + URLs. Body matches use literals from our
  own code (`/TradeTunnel connection lost/i`, `/WSoverHTTP request timed
out/i`). URL match is the literal substring `exchange_proxy` only —
  **don't widen this** without confirming a real run produces a URL the
  narrow form misses, since false positives mis-bucket benign 4xx/5xx
  on non-proxy tealstreet endpoints as transport flaps.
- **`transportFailureUrl`** — path of the first request that triggered
  the classification (query string stripped). First-match-wins; later
  failures don't overwrite.

The Slack reporter at `cypress/scripts/slack-e2e-report.js` reads
`ex.transportFailureReason` to bucket as `transport-flap` (precedence
above `partial`/`clean-failure`), prepends a `:satellite_antenna:`
marker on the exchange line, and collapses per-test exchange-error
spam into a single `Transport: <reason>` line.

## API log / observability pipeline (verbose → `requestsLogAtom` → artifact)

Each spec dumps captured exchange traffic to
`cypress/logs/<exchange>-api.log`, uploaded as the
`e2e-api-logs-<exchange>` artifact (3-day retention). The flow:

1. **Cypress auto-enables verbose.** `src/atoms/dev.atoms.ts` flips
   `enableRequestResponseAndPrivateWsLogAtom` on under `window.Cypress`
   and exposes `window.__getRequestsLog()` / `__clearRequestsLog()` /
   `__getStoreSnapshot()`. `utils/shim/worker/workerFactories.ts`
   pushes a `verbose` frame to the **hosted exchange-server** (CI runs
   with `CYPRESS_useExchangeServer: 'true'`, set in the e2e workflow) so it
   stops dropping `grafana-*` events at the bridge. That env bypasses
   feature/toggle gates and the early `isLoggedInAtom` hydration gate:
   account registration can race ahead of auth atom hydration during test
   startup. The test browser still must be logged in because
   `ExchangeServerWorker` fetches `/api/proxykey` for the socket auth URL
   and retries briefly at startup before failing. See
   `services/exchange-server/CLAUDE.md` → "Verbose mode".
2. **`exchangeAccountController` is the single writer** into
   `requestsLogAtom` (in-memory Jotai).
3. **`afterEach` flushes** via `window.__getRequestsLog()` →
   `cy.task('writeApiLog', …)` (spec.cy.ts ~1190) → the per-exchange
   file, then **clears the atom**. On a failing test it also stores a
   relevance-filtered subset (error-severity, 200-with-error-code
   bodies, last trading-endpoint + WS frames, store snapshot) into the
   results JSON for the Slack report + Cypress Cloud command log.

**What `requestsLogAtom` captures** — its `RequestsLogEntry.type`
values (`dev.atoms.ts`): `rest | privateWs | privateWsSub | httpRequest
| httpResponse | exchangeError | exchangeWarning`. Entry shape is
`{timestamp, type, severity, message}` — REST + **private (account)
WS** + emitted errors only. Bounded at ~500 entries (enforced where the
controller pushes); balance polls fill it fast. (The `'diagnostic'`
type value is reserved for the **separate** diagnostic ring below, not
`requestsLogAtom`.)

### Diagnostic log (connection-health + data heartbeat)

A **separate** ring, `diagnosticLogAtom` (`dev.atoms.ts`), carries
`type: 'diagnostic'` entries so the REST/WS spam in the 500-capped
`requestsLogAtom` can't evict the "no data / yellow connection" story.
`exchangeAccountController` is the single writer:

- **Connection-state transitions** — logged on _change_ in the
  `connection-state` worker-message handler (the hosted-server WS state
  behind the yellow connection indicator), e.g.
  `connection-state connected -> reconnecting account=…`.
- **Data heartbeat** — a 2 s self-rescheduling `setTimeout` loop
  (started/stopped with the `enableRequestResponseAndPrivateWsLogAtom`
  toggle, so it's inert in prod) samples per-account `tickers=<count>`,
  `lastDataAgeMs`, and `loaded={…}`. This answers "did market data
  arrive, and when did it stop?" — the question the popover/screener
  timeout hinges on.
- **Server health (Phase B)** — the hosted exchange-server forwards a
  `diagnostic:server-health` frame (per-worker conn count, WS buffered
  bytes, event-loop lag, mem, uptime) into this same ring, joined by
  `connId`, so server load sits next to the client signals. **Hard-gated
  server-side** to diagnostic builds (`TEALSTREET_DIAGNOSTIC_BUILD=1`) —
  see `services/exchange-server/CLAUDE.md` → "Verbose mode".

The cypress `afterEach` dumps this ring to the same
`<exchange>-api.log` file in its own `[DIAGNOSTICS]` section
(`window.__getDiagnosticLog()`), then clears it — independent of the
requests-log branch, so it survives even when `requestsLogAtom` is
empty. The dev Requests drawer does **not** render this ring (it reads
`requestsLogAtom`); it's artifact-only.

### Observability boundary — still NOT captured

- **Raw public market-data frames** (individual ticker / orderbook
  messages) — the heartbeat reports ticker _count + age_, not the
  frames themselves, which is enough for "did data arrive?" without the
  volume.
- **Server load on a non-diagnostic build** — `diagnostic:server-health`
  only emits when the exchange-server runs with
  `TEALSTREET_DIAGNOSTIC_BUILD=1` (the e2e deploy). Against a prod
  exchange-server it's absent by design (worker internals aren't
  exposed), so the `[DIAGNOSTICS]` section then carries only the
  client-side connection-state + heartbeat.

For anything beyond those logs you still want the **Cypress Cloud
video** (recorded via `--record` when `CYPRESS_RECORD_KEY` is set; the
per-exchange run URL is in the result JSON `runUrl`). GH artifacts carry
only `e2e-results-*` (1-day) and `e2e-api-logs-*` (3-day) — **no
screenshots/video**.
