# TS safe-cex runtime deletion: inventory

Prep for the final phase of the approved migration. This document deletes no
code. It lists what becomes dead after each of two prerequisites, what still
references it, and what breaks if it goes early. Baseline: SDK master
`a5e40597cf6`, plus a read-only grep of tealstreet-next master `65e086a3c90`
("NX"). LOC is `wc -l` of non-test sources; test files are listed separately
and go with their subject.

## A. Every core family is frame-fed

Once every family a keyed mirror can hold is in `FRAME_FED_VENUES`, every
mirror builds a frame-fed page engine (`frameFedReady` resolves true). Manual
writes are signed and sent on the page, so the forwarded-write half of the
mirror is dead. Reads still go to the data source; that half stays.

| Code | LOC | Still referenced by | Breaks if deleted before A |
| --- | --- | --- | --- |
| `exchanges/base.ts`: the write entries of `WORKER_FORWARDED_METHODS` (place*, updateOrder, cancel*, nuke*, setLeverage, setAllLeverage, changeMarginType, changePositionMode), plus `WORKER_WRITE_METHODS` and `workerCallClientOrderIds` (lines 314-420) | ~60 of 107 | `rust.exchange.ts` `forwardToWorker`; `ExchangeWorker.ts:1009-1013` (an interrupted worker write reports `outcome: 'unknown'` with caller IDs); `CallMethodHandler.ts`; `rust.mirror-forwarding.test.ts` | Manual writes on a family that is not frame-fed, and the unknown-outcome reporting for an interrupted forwarded write. |
| `exchanges/rust/rust.exchange.ts`: `FRAME_FED_WRITES` (17 names, :308), the write side of `forwardToWorker` (:1335-1360), the non-frame-fed fallbacks (`changeAccountTier` :602, `scheduleCancel` :2417, `syncAccountState` :1586), and store-payload capability loading for a mirror without frames (`loadWorkerCapabilities` :496-531) | ~120 | Only this file and its tests (`rust.frame-fed-mirror`, `rust.mirror-forwarding`, `rust.mirror-hosted-init-fields`) | Same as the row above. Capability loading also serves store-payload mirrors (no frames), which A removes. |
| #2506 settings forwarding: setLeverage, setAllLeverage, changeMarginType, changePositionMode and changeStpMode whole to the worker (`RUST_FORWARDED_METHODS` keeps `changeStpMode`) | included above | `rust.mirror-forwarding.test.ts` (the "worker" column) | Settings writes on a family that is not frame-fed. OKX's removal waits for ddi's go after the release is verified live. |
| `FRAME_FED_VENUES` gating and the `frameFedSupported`/`construction().frameFed` fallback (`startFrameFed`, :929-940) | ~30 | registry `FRAME_FED_VENUES`; `rust.frame-fed-mirror.test.ts` | Collapses to "always frame-fed"; the list becomes unnecessary. |
| `sdk-rs/hosts/server/src/methods.rs`: write translations for browser-forwarded writes (placeOrders through refreshAccountSettings, :362-566; scheduleCancel and nuke*, :642-682) | ~230 of 1,575 | COSIGNED/DELEGATED browsers today; server-side algos call the engine directly, not through `translate` | COSIGNED/DELEGATED manual writes before their families are frame-fed. Also check script-api and CLI remote callers before removing. |
| NX `apps/web` callers of writes through `callMethodInWorker` / `callMethodInWorkerFireAndForget`: 13 + 5 files, including `useLeverageSlider`, `useMarginType`, `order/cancelOrders`, `order/updateOrder`, `useSymbolMetadataUpdate`, `module-frame/accountHooks` | n/a (NX) | NX only | They send writes by name to the worker. Each must call the page engine (`getExchange`) before the SDK write list can shrink. Reads may stay on `callMethodInWorker`. |

Stays after A: `callWorker`, `CallMethodHandler` for reads (history, fills,
closed positions, leverage reads, capability record, `refreshAccountSettings`,
`stopAccountAlgos`), frame forwarding (`webworker/frameForwarding.ts`), and
`ExchangeWorker` as the LOCAL data source.

## B. LOCAL algos run in Rust

Once chasers, TWAPs and CLI exec run in the Rust engine host, the worker no
longer needs a TS store or the TS algo shell; it is a frame source only.

| Code | LOC | Still referenced by | Breaks if deleted before B |
| --- | --- | --- | --- |
| `webworker/chaserOrder/`: Manager 1,043, Instance 281, Options 66, State 6 | 1,396 | `ExchangeWorker`, `ChaserOrderHandler`, `ChaserOrderPostprocessor`, `stopAccountAlgos`, `store/store.base.ts` (chaser state types). **cli-core** `ChaseCommand`, `ChasersCommand`, `KillCommand`, `TasksCommand` (SDK and NX) | Browser chasers, and **CLI chasers, which run in-process in Node**. The CLI needs the Rust algos too, or these classes stay. |
| `webworker/twap/`: Instance 365, twapLock 69, Manager 49, twapRestoreRefusal 47, Options 44, State 28, Status 6 | 608 | `ExchangeWorker`, `TwapHandler`, `TwapPostprocessor`, `stopAccountAlgos`; **cli-core** `TwapCommand`, `KillCommand`, `TasksCommand`; **apps/cli** `repl.ts` | Browser and CLI TWAPs, and TWAP restore after reload. |
| Algo handlers and exec: `ChaserOrderHandler` 40, `TwapHandler` 110, `CommandHandler` 63, `commandExecutor.ts` 33, `stopAccountAlgos.ts` 18 | 264 | `ExchangeWorker`; `TelgramWorker.ts`; **cli-core** `worker-host/router.ts` (`registerCommandExecutor`); NX `StoreWorkerEntry.ts`, `services/exchange-server/register-command-executor.ts` | The browser terminal's `exec`/`exec-result` bridge (cli-core commands run in the worker), Telegram commands, and the nuke's `stopAccountAlgos` (rust.exchange.ts:2030). |
| Algo post-processors: `ChaserOrderPostprocessor` 32, `ChaserOrderEnrichmentPostprocessor` 58, `TwapPostprocessor` 31, `TwapOrderEnrichmentPostprocessor` 31 | 152 | `postProcessors/registry.ts` | Algo rows and order enrichment in the UI. |
| Worker TS store and shell: `ExchangeWorker.ts` 1,312, `store.webworker.ts` 644, `StoreWorkerPayload.ts` 177, `BaseHandler` 184, `CallMethodHandler` 160, `OHLCVSubscriptionsHandler` 118, `OrderBookSubscriptionsHandler` 112, `UiStateHandler` 101, `UpdatePositionsHandler` 92, `ThrottleManager` 50, `DevHandler` 23, `TelgramWorker.ts` 95, `perfFixture.ts` 141, `workerErrorSerialization.ts` 25 and the small type files | ~3,250 | NX `apps/web`: `StoreWorkerEntry.ts`, `ExchangeServerWorker.ts`, `workerFactories.ts`/`createStoreWorker`, `exchangeAccountController.ts`, `dev/perfFixture.ts`, `sanitizeWorkerSentryData.ts`, `mirrorProjection.ts`, `optimisticOrderTtl.ts`. **NX `services/exchange-server`**: `server.ts`, `public-exchange-manager.ts`, `public-virtual-worker-context.ts`. SDK `safe-cex-server/publicExchange/tickerCache.ts` | The LOCAL worker itself (its data-source role must be reimplemented as a Rust frame source first), the **TS hosted exchange-server**, which must be retired in favour of `sdk-rs/hosts/server` first, and the public ticker cache. |
| Non-algo post-processors: `DisplayedPositionsPostprocessor` 349, `MessageHandlerStatsPostprocessor` 62, `registry` 44, `OrdersRiskProfilePostprocessor` 41, `DisplayedMarginPostprocessor` 34, `ExchangeDependentPostprocessor` 22, `BasePostprocessor` 19 | 571 | `ExchangeWorker`; `utils/risk.ts`; NX `snapshot/BaseTaskPostprocessor.ts`, `registerSnapshotPostprocessor.ts`, `hostedFrameClient.ts`, `eagerExchangeWarmStart.ts` | Their **outputs are UI state**: displayed positions and margin, `store.ordersRisk` (chart `OrderLineManager`, `RiskIndicator`, module API) and NX snapshot upload. These need a new home (page side or Rust) before deletion; they are not dead just because the worker is. |

Stays after B: `store/store.base.ts` (1,094) and `store/frameApplier.ts` (800).
They are the main-thread UI store the page engine's frames fill. `cli-core`
also stays as the CLI. Deleting the TS venue reference implementations under
`__tests__/reference` is a separate step (RUST_REWRITE.md phase 9).

## Order and gates

1. Finish A: every family frame-fed. Then move the NX write callers to the
   page engine, then delete the A rows, then remove `FRAME_FED_VENUES`.
2. Before B: CLI chasers and TWAPs on Rust (or keep the TS classes for the
   CLI), the TS exchange-server retired, and new homes for post-processor
   outputs.
3. B: replace the LOCAL worker with a Rust frame source, then delete the B
   rows. The open B blockers are the three cli-core and NX consumers named
   above.

## Proposal: where post-processor outputs live (blocker 3)

Proposal only, not yet approved. It covers `displayedPositions`,
`displayedMargin`, `ordersRisk` and the NX snapshot upload.

**Run the independent post-processors on the page, over the page engine's
store. Hosted mode already does this.** NX `hostedFrameClient.ts` builds
`independentPostprocessors` per account on the main thread (:68-72) and runs
them on every hosted frame (:192). The snapshot upload is already a registered
independent post-processor that runs there. LOCAL would use the same path, so
every runtime computes these fields the same way and in the same place.

- **What moves:** `BasePostprocessor`, the `independentPostprocessors` list
  with `registerIndependentPostprocessor`, and the five independent classes.
  The classes are DisplayedPositions, DisplayedMargin, OrdersRiskProfile and
  the Chaser/Twap enrichment pair. They move out of `webworker/` (for example to
  `src/postProcessors/`) unchanged. They are pure functions of `StoreData` plus
  account context, which is the registry's own definition of
  "independent".
- **Where they run:** after each frame the page engine applies, throttled to
  the worker's current 250 ms (`tuning.dataUpdateThrottleMultiplier`). They
  write the derived fields into the page's `store.base`. `FrameApplier` never
  writes those fields, so a later frame can't overwrite them. Run order stays
  DisplayedPositions before DisplayedMargin.
- **State:** DisplayedPositions keeps 1m/5m PnL baselines per position
  (`TimeLimitedQueue`, :136-137), and the OrdersRisk/DisplayedPositions map
  caches are per instance. So there is one instance set per account on the
  page, created with the page engine and dropped with it. That matches hosted
  mode. A page reload loses the baselines, as a worker restart does today.
- **Snapshot upload:** no change. It stays an NX-registered independent
  post-processor (it posts to Tealstreet routes, which the OSS package must
  not do). It already runs on the main thread in hosted mode.
- **Cost:** the main thread takes on what the worker did at 4 Hz. Hosted users
  pay this today; measure LOCAL on the largest account (Bybit linear) before
  cutover. If it is too slow, move `calculateOrdersRisk` first, either to a
  Rust derivation or by keeping it off the render path.

**Not chosen: compute them in Rust and ship them in frames.** That would port
`utils/risk.ts` and the displayed-position derivation, add frame sections, and
grow every family's wasm, where several have only about 10 KB of headroom. It
would also leave the snapshot upload on the host anyway.

**Exchange-dependent post-processors:**
- `activeChasers` and `activeTWAPs` come from the algo engine. Once algos run
  in Rust (B), their state arrives in frames, and the Chaser/Twap
  post-processors go.
- `MessageHandlerStatsPostprocessor` measures the TS worker's message handlers.
  With the worker gone it has nothing to measure; the engine's `getHealth()`
  counters replace it.
