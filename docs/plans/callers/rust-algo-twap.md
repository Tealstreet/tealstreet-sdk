# Rust TWAP core

`sdk-rs/algos/twap` (`safe-cex-algo-twap`) ports the TS worker's TWAP
(`TwapHandler`, `TwapInstance`, `TwapManager`, `TwapPostprocessor`,
`TwapOrderEnrichmentPostprocessor`) as a clock-driven state machine for the
native exchange server. Nothing calls it yet.

## Obligations

- Native server: per account, build one
  `safe_cex_chaser::ChaserFactory::new(account_cx.clone())`, take
  `let chasers = chaser_factory.book()`, and build
  `TwapFactory::new(&account_name, account_cx, Rc::new(chasers))`, where
  `account_name` is the `accountName` the account's clients and `activeTWAPs` use. Register both factories
  in that account's `Registry`. Pass the account's `AlgoCx`, never a
  socket's: TWAPs keep running when the socket that started them closes.
  `TwapFactory` emits `twap:stateUpdate` and `error` (the message string,
  which the web toasts; TS used `log`, which reaches only the exchange log) and
  exposes `active_twaps` and `order_owner` for the store's `activeTWAPs` and
  `extra.isFromTwap`.
- Account disposal: run, in order,

  ```rust
  tw.terminate_all();
  chaser_book.shutdown(reason).await;
  account_scope.dispose();
  ```

  `terminate_all` is synchronous: it stops every schedule and slice deadline
  and emits the final `twap:stateUpdate`, and it spawns nothing. Chase slices
  live in the chaser book, and `shutdown` awaits their order cancels;
  `ChaserBook::terminate_all` only spawns them, and disposing right after
  aborts them. A direct slice placement already sent to the engine is not
  awaited, and TS `terminate` never cancelled direct-slice limit orders
  either, so those stay resting.
- Exec `twap` command: take a clone of the account's `TwapFactory` (clones
  share one book) and call `start(TwapOptions) -> Result<String, String>`,
  which returns the new id or the refusal text, and `stop(&id) -> bool`. Use
  `active_twaps(account)` for `twap list` and prefix matching. Typed calls
  emit `twap:stateUpdate` like the ops do, but return refusals instead of
  logging them.
- Web client: unchanged. It keeps persisting `twap:stateUpdate` and replays it
  as `twap:state` on connect.

## Behaviour callers must know

- Client input is bounded before any slice is built: `size > 0`,
  `duration > 0`, `1 <= ordersCount <= 1000` (the web's `TWAP_MAX_ORDERS`),
  `randomnessPercentage` and `timeoutPct` in `[0, 100]`, and
  `chaserDelay >= 0`. Restored entries must also keep `ordersSizes` within
  `ordersCount`, with finite values. A refusal sends an `error` and builds
  nothing; a bad restore entry is skipped. An account runs at most 20 TWAPs
  (`MAX_TWAPS_PER_ACCOUNT`): further starts are refused and further restored
  entries are skipped, until one completes or stops.
- A `twap:start` or restored row whose `accountName` is not the book's
  account is refused with an `error`: it would trade on this account yet
  never be listed in its `activeTWAPs`.
- `twap:state` replays one tab's `sessionStorage` (web `TwapWorker`), which
  can be stale, copied into a duplicated tab, or re-sent on every account
  load. Unlike TS, a restore is refused when the entry is completed, idle or
  out of slices; when its remaining slices exceed `size` or, with executed
  ones, `ordersCount`; or when it is running and its schedule lapsed (now is
  past `nextOrderAt` plus one slice interval with randomness, plus
  `SCHEDULE_GRACE_MS`). An id the book holds, including one still settling
  its last placement, is skipped, and an id that completed or stopped on
  this account (the last `RETIRED_IDS_KEPT`) is never revived. These guards
  live in the account's book: they do not survive the book's disposal or
  span servers. A restored running TWAP places at most one overdue slice,
  then paces at its interval. A paused one places nothing until resumed.
- A slice whose placement outcome is unknown, or whose order ids cannot be
  read, still advances the TWAP, as in TS. Its log says the order may be live,
  and no later slice assumes it failed.
- TWAP ids come from a non-cryptographic generator and are addressable only
  inside their account's book. Never resolve a TWAP id across accounts.
