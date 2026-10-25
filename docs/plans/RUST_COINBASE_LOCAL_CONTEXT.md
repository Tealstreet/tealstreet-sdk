# Coinbase local command custody

Plan first, 2026-10-24. Owner codex-hcmq3f; fresh branch
feat/rust-coinbase-public-local-context, refreshed by plain merge of actual
77f124089477a6aed502d9b02aa7f08752b7bfa5. This follows
Polymarket #1995 and Kraken #2000; only independently unblocked local paths are
selected. No capability, signing, account, queue or shared engine edits.

## Corrected inventory and source gates

The accepted lane inventory records Coinbase 31 fingerprints / 34 occurrences,
with a provisional 15/15 public/local group. Source re-verification finds five
of those occurrences need additional custody composition. The actual target is
10 fingerprints / 10 occurrences, leaving 21 / 24. All five newly held entries
remain byte-exact in the manifest and are not hidden behind new spelling.

| Held source branch | Exact reason |
| --- | --- |
| public.rs instruments send | Its catalogue callback calls public_pending, which manually invokes public_command for deferred commands. The failure path also numerically completes deferred candle waiters. A managed Background delivery cannot lend those commands completion rights; actual #1896 readiness adoption must precede this conversion. |
| public.rs candle interval/symbol/range refusals (three sites) | Each also runs from public_pending during a catalogue callback. Fresh admit_http_command there would be a forged entry and would strand deferred commands. Keep these on the same #1896 continuation adoption. |
| public.rs daily seed send | Shares rest.rs's 256-entry FIFO and 334 ms scheduling with unadopted signed requests. Direct managed I/O bypasses native pacing; delayed preparation plus unblocked raw siblings can reorder release. Exact shared queue/release composition is required before conversion; no local clock, polling wake or route workaround. |

These are source gates, not claims of a new engine vulnerability. Core is asked
to confirm/refine the queued-Daily boundary before any runtime proposal for it.
The other 19 previously blocked occurrences retain signed JWT identity/admission,
background aggregate, history/journal and deferred command obligations. No public
or private HTTP path is migrated in this local-only slice.

### Candle path gates (overseer, 2026-10-25)

The candle read, `public.rs:476`, and its three refusals at `public.rs:1574`,
`1578` and `1586` need both gates before any managed conversion. Neither is
enough alone:

1. **Catalogue readiness.** A candle command deferred behind the instruments
   catalogue is dispatched from `public_pending` inside that catalogue's
   callback. Its custody must come from #1896 Preflight readiness; a fresh
   `admit_http_command` there is a forged entry.
2. **Shared engine pacing and ordering.** The candle send shares `rest.rs`'s
   256-entry FIFO and 334 ms pacing with the unadopted signed requests. A
   managed candle read needs one engine composition that paces and orders the
   legacy queue and managed requests together; no venue-local clock or wake.

The core pacing composition (2) is on the engine backlog behind dazzy3's S0-S2.

## Selected native branches

- lib.rs FetchAccount success/error keeps the exact selected private portfolio
  identity JSON or unavailable error; capability/AfterLoad routing is untouched.
- history.rs initial validation error keeps active-request limits, query support,
  credential, limit, timestamp/range, order/product identity and family validation
  precedence. Valid histories retain native CursorHistory/Completion, paging and
  queued signed requests. No callback is converted into a new initial command.
- public.rs startup queue-full rejection happens before enqueue at genuine entry.
  Preserve the warning notice and the 256-command cap; deferred command storage,
  callback drain, retries, ready behavior and terminal failure loop remain exact.
- trading.rs unsupported account mutation, cached leverage success/unavailable,
  account-not-ready, queue-full and prepare-job refusal keep native messages,
  JSON, typed metadata and validation order. Read results remain Read; mutation
  refusals remain Mutation. Valid jobs, queue advancement, reconciliation/fill
  reducers, private signing and writes remain legacy and outside this slice.

A small native projection delegates to existing genuine initial command entry,
with no local lifecycle/owner registry. Capture no synthetic command, Context,
credential, attempt, accepted fact or retry. None/Expired caller receipt uses
reject_unstamped_command/reject_expired_command with the native operation.
Remaining uses admit_http_command with no credential, no pacing, Never retry and
an explicit 20000 ms declaration (no physical I/O is queued); immediately use
done_context/fail_context. Rejected entry already emitted its one terminal;
Revoked grants no terminal or numeric fallback. Existing native local errors,
including NotSent fields on pre-I/O refusals, are passed intact.

No successful keyless Mutation appears in this selected set. If one is found,
use landed #1989 only for a genuine Setting rather than Read-casting or fake
write evidence. The frozen
#1896 branch is not consumed by this independent local adoption.

## Original controls and verification

Commit this plan before original test controls, then run those controls on
unchanged production. Use genuine Engine command entry; an invalid native clock
should produce the shared typed InvalidInput refusal before local completion,
with live-clock controls proving the same branch's exact native message/JSON.
Exercise no-I/O mutation refusal, unavailable/cached leverage, initial history
validation and the real 256-entry startup queue-full refusal. Verify no HTTP,
Store mutation or native queue advancement. Keep settled same-ID reuse and
private/discovery/deferred working controls. Compilation errors, inaccessible
private fields or a missing API do not count as assertion REDs.

After cutover, original assertions must pass unchanged. Prune exactly the ten
selected fingerprints/occurrences, keeping every held entry and other-family map.
Run scoped Coinbase native tests, strict Clippy, fmt, both request-context checks
and bans. Independent source, caller and full-map reviews precede readiness;
CI supplies full replay, WASM and size proof. The overseer explicitly requires
family size/headroom reporting: an over-budget draft stays frozen for fe7lgm's
shared entry shim, with no venue workaround. No merge without an explicit window.

## Reverified source map

- SELECTED `venues/coinbase/src/history.rs:318` — `0451c29993291234cad6e42a586a776c0df200276aac899eaf08c1753d2e83fb`: cx.fail(command, e)
- SELECTED `venues/coinbase/src/lib.rs:188` — `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548`: cx.done(command, data)
- SELECTED `venues/coinbase/src/lib.rs:189` — `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`: cx.fail(command, error)
- HELD `venues/coinbase/src/public.rs:243` — `3d0b652732f625e52ffcca8e7d59f829243e0666aa727667ad5588e05ec8b6e5`: self.request( cx, Method::Get, INTX.into(), None, Some(ArrayAt::Root), Pending::Public(pending), )
- HELD `venues/coinbase/src/public.rs:476` — `a7a039a5f67c9622fe5c45051ebd95a0b70818fb43f77e586184a00a8b5420f1`: self.request( cx, Method::Get, path, None, Some(ArrayAt::Field("candles")), Pending::Public(pending), )
- SELECTED `venues/coinbase/src/public.rs:1448` — `60b349f5e304a70150b0df808e057d528d77e7f2b4e016c37416a7626a3a83e1`: cx.fail(command, "Coinbase public startup command queue is full")
- HELD `venues/coinbase/src/public.rs:1574` — `e347074f96f429e946a8dd06ad52ec6468d87d13ecf9b0de549424317be7a975`: cx.fail(command, timeframe.unsupported("Coinbase"))
- HELD `venues/coinbase/src/public.rs:1578` — `2ff357e755f38d4c8de909f3b5b32d2c683cb185942daa383330a3f4cc0d5532`: cx.fail(command, "Unknown Coinbase market")
- HELD `venues/coinbase/src/public.rs:1586` — `e7eb9e6b80ae49514fa1e48353268acf5121932b10de4f05cdbd6ced0cc8f58c`: cx.fail(command, "Invalid Coinbase candle range")
- SELECTED `venues/coinbase/src/trading.rs:1113` — `bd1bdc3f1623f5957810ab1aeb3584ede2872387055a238ea00e3d22cd37f5fc`: cx.fail(id, not_sent("Coinbase does not support account leverage, margin or position-mode mutations").kind(ErrorKind::Unsupported))
- SELECTED `venues/coinbase/src/trading.rs:1124` — `263d0619925b412640098c8ec05ba6f5b8211f6e45026f36e4e3a1fd07934a15`: cx.done( id, encode(&Leverage { buy_leverage: setting.leverage, sell_leverage: setting.leverage, }), )
- SELECTED `venues/coinbase/src/trading.rs:1132` — `479442329d5a2562b05c949ca383d819acffc77d8541e1296588b558d4200b00`: cx.fail( id, VenueError::new("Coinbase market settings are unavailable"), )
- SELECTED `venues/coinbase/src/trading.rs:1147` — `414b9feff64a0a0bcbcc856631dcf20976a5797d25a40a434f3b710206300bee`: cx.fail(id, not_sent("Coinbase account is not ready for trading"))
- SELECTED `venues/coinbase/src/trading.rs:1152` — `d6aab93d8c0593f58c17fbc2fba81bb939f0d498ba488ca2bac1d11287e68324`: cx.fail(id, not_sent("Coinbase trading queue is full"))
- SELECTED `venues/coinbase/src/trading.rs:1160` — `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5`: cx.fail(id, not_sent(error))

## Refreshed original-source receipt

At plan/fixture merge `620df9f954c` on actual `77f12408947`, Coinbase production
is byte-identical to the landing. The original `local_context` target executes
five assertion REDs and two working GREENs, zero ignored. Invalid-clock entry
still reaches native portfolio/history/startup refusals or unsupported Mutation;
cached leverage emits a success instead of the expected InvalidInput refusal.
The two finite-clock controls preserve exact native JSON/error metadata,
settled command-ID reuse, and the real 256-entry startup warning/capacity path.
This is an executed assertion failure, not a missing API or build failure.

## Local adoption verification

The ten selected sites now use one local projection through genuine initial
Read admission or keyless Mutation refusal. None/Expired caller waits use the
cause-aware shared rejection functions; there is no numeric fallback. The
original seven controls are unchanged and all pass. The scoped Coinbase native
suite passes 209 tests, zero ignored, preserving public/private discovery,
queued REST, history, account and trading controls. The exact inventory removes
1 history, 2 lib, 1 public and 6 trading occurrences; all 24 remaining Coinbase
occurrences and every other-family entry are preserved. Full CI and family
WASM size/headroom remain gates before READY.
