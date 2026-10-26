# Orderly provider-signed reads (O1) and trading writes (O2)

Owner: claude-kuhwxi (overseer assignment, 2026-10-26). Overseer: claude-ddi2il. Reviewer: claude-dazzy3, claude-55ao7t or claude-4yjlw0. Landing: claude-8x5tm1 gives an exact-tree GO, because the registry changes.

Base: `feat/rust-rewrite` `393b7668194`. Area plan: [RUST_ORDERLY_REQUEST_CONTEXT.md](RUST_ORDERLY_REQUEST_CONTEXT.md). Precedents: BloFin #2157 (Preflight FIFO, serial Plans) and #2188 (socket login on dispatch, raw options retired), and Bitrue's funnel plan.

## Why two slices

Orderly signs locally: the venue decodes a raw Ed25519 seed (`sign::Credentials`) and builds the headers itself. The registry installs no provider, and the exported `sign::Http` recipe was unused outside its byte-for-byte test. A managed private request needs a dispatch `SignStep` backed by an installed provider, so none of the 18 request sites can move until the provider exists.

- **O1 (this PR).** The provider, the account reads, history and the socket auth.
- **O2.** Trading writes on Preflight roots and Plans, then the raw key and seed and `sign::Credentials` are deleted.

O1 keeps the local seed only for the legacy trading funnel. This is a recorded interim, and O2 deletes it.

## Inventory

| Manifest | File | Base | After O1 | After O2 |
| --- | --- | ---: | ---: | ---: |
| request | `account.rs` | 4 | 0 | 0 |
| request | `history.rs` | 6 | 0 | 0 |
| request | `trading.rs` | 8 | 8 | 0 |
| signing | `account.rs` | 3 | 1 | 0 |
| signing | `lib.rs` | 9 | 9 | 0 |

In `account.rs`, the two `matches!(request, Request::Info)` checks become the `Request::is_info` predicate. In history, the `format!` sites read the page through a `read` binding. Neither is a rename to reach zero: both functions now send through managed legs. The signing manifest is pruned only. The inline margin-mode unit tests move to `tests/unit/margin_mode_rows.rs` (the existing `public_seed_custody` pattern), so their trusted provider setup stays out of `src`.

## O1

- **Provider.** `orderly_signer` installs the seed as declared base58 with `Algorithm::Ed25519`. The presented key is native's `ed25519:`-prefixed public key, and either string may carry the prefix, as natively. A public-data-only or keyless account installs nothing. `Options.identities` carries the refs.
- **Account reads.** Info, margin modes, positions and the open-order pages are Background `SelectedRead`s with the Primary role (Orderly has one key), 20 s each, with no retry policy. The native revision and inflight joins, the startup retry and the 4xx Info deactivation are unchanged.
  - Each delivery validates its Background custody before any row or Store effect. A stale leg only closes its Context.
  - A malformed stream, a transport failure and a signing refusal are checked the same way before `account_failed`, so a superseded read never clears a newer read's inflight flag or re-arms the startup retry (dazzy3 F1).
- **History.** One command Read root per query, admitted after native validation, and an expired or unstamped receipt is the shared typed refusal.
  - One signed leg is issued per page. The rows of a page are mapped as they stream and staged through the `ReadCursorHistory` admission only after the delivery validates at the response, in native order and under the same limit.
  - The funding spacing timers park the root between pages, and the caller's wait bounds the whole walk.
  - Public trades read unsigned under the command Context.
  - A transport loss keeps its typed kind (`Network`). A signing refusal is strict, even after accepted pages.
- **Socket auth.** `sign::WsAuth` signs the whole-millisecond release stamp's decimal text. The frame is native-exact: `{"event":"auth","params":{orderly_key,sign,timestamp}}`. With no Primary identity the socket closes without a frame.
- **Deferred trading tail (O1 interim, O2 deletes it).** The engine refuses any legacy completion inside a managed delivery, so a protective wait that an account snapshot confirms is released by a zero-delay venue timer armed from that delivery. It is armed only while a wait or fence exists, fires once, and its control is `tests/deferred_trade_tail.rs`. Without it, a confirmed pair never completed, which was CI's replay timeout on the first push.
- **Signing at release.** Every signed request's stamp is its release time, the same instant native used whenever no queueing intervened.

## O2: trading writes

- **Entry and FIFO** (BloFin #2157 shape).
  - Each trading command owns a `begin_preflight(WriteAfterReads)` (Primary) before validation, so every refusal is its own NotSent terminal.
  - A keyless account, or Bybit-only controls, is refused under the command's own Context before any Preflight.
  - Native's queue (cap 100) parks Preflights in FIFO order, and the front is planned again against the current Store when it wakes. A wake past the caller's wait is the typed timeout, NotSent.
- **Bulk cancellation.** It supersedes queued order mutations on its scope. A parked Preflight fails only in its own ready custody, so each superseded one is woken and fails NotSent with native's text. The live job on the scope sends nothing more, and a paused one ends at once with native's error. The cancellation runs next, at the front of the FIFO.
- **Jobs.** Every ordered job (placement, protective pairs, amend, leverage, margin type) is a streamed serial Plan, and each next call comes from the previous call's DATA. A placement's conflicting-order cancel is narrowed to a Cancel leg.
  - FetchLeverage keeps its native FIFO place under a `ReadOnly` Preflight: one Read Plan (`admit_plan`, Read policy) settled by `done_read_plan_after_data`.
  - Bulk cancellation's two family DELETEs are a complete Plan with `independent_calls`. A failed leg is settled and the next one still goes, as natively.
- **Protective pauses.** After a pair write (or a parent cancel that precedes a rebuild) whose legs aren't yet visible, the Plan parks in `serial_plan_condition_wait`, bounded at 20 s. It is released from `trade_reconcile` once the stream or a snapshot shows the legs, or shows the root as terminal.
  - **D1** (dazzy3): a job that ends on such a pause issues one Primary-signed `GET /v1/algo/orders?status=INCOMPLETE&page=1&size=500` (the native account poll's own listing) as its final Read call, and `done_plan` follows only when the waited legs are listed or terminal.
  - Wait expiry, a listing that lacks a leg, and any failure of that read call `inherit_plan_uncertainty` (#2205) before `fail_plan`. The terminal is Unknown with the accepted IDs and native's "protective state is unconfirmed" text, and the scope stays fenced until reconciled.
- **Write facts.** Successful writes record `Accepted` with their order IDs.
  - Delivered failures are classified by the cited `CODES` table (orderly.network error codes). A cited refusal is Rejected. -1000, -1007, -1011, -1012 and any unlisted code are Unknown, and an unknown write reconciles the account, as a lost one does.
  - 408/5xx and an unusable 2xx ACK are `http_write_unproven`.
- **Retirement.** `capture_http_reconciliation` correlates by endpoint.
  - `on_http_owner_retired_preflight` keeps the call in flight, and `settle_retired` restores it as lost (uncertain rows, a reconcile) at the next callback or `reconcile_uncertain`.
  - `order-locks` is linked so that hook runs. A complete fresh orders snapshot reports `orders_reconciled`.
- **Raw seed retired.** `Options` no longer has the key or seed, and `sign::Credentials`, its erasure and the `Sensitive` wrapper are deleted. The registry's shape check is the stateless `sign::keys_valid`, and a mismatched key and seed pair is still caught only by the venue (dazzy3's nit, deferred: every fixture uses an unpaired key).
- **O1 interim deleted.** The zero-delay deferred reconcile is removed: a condition-wait release only schedules, so it is legal inside a managed delivery.
- **Inventory after O2:** request 18 → 0 and signing 12 → 0 for Orderly.
- **Review record (dazzy3's O2 lows).**
  - **L5.** `orders_reconciled` follows the regular-orders snapshot alone. The engine's lock fences placements, which are regular orders; positional pairs stay behind the venue's own detached fence until an algo snapshot satisfies it.
  - **L6.** A read discarded after a later write's revision bump is re-read at the next poll tick, as native. Re-polling at the discard changed which read a native recovery control answers, so it is left for a separate change.
  - **L7.** A bulk cancellation is parked at the FIFO front even with 100 queued (native: it supersedes rather than queues). If that park fails, its terminal is NotSent and the superseded commands keep their own NotSent terminals.
- **Replay:** the private broker tapes (orderly and woofi on mainnet, kodiakfi on testnet) match the base wasm exactly, 912/912 declared known failures.
- **Engine features:** `condition-waits`, `http-plans`, `order-locks` and `serial-plans`. The combined build already links all four through other venues, so only Orderly's family grows. CI measures it.

## Controls (O1)

`venues/orderly/tests/provider_signing.rs`:
- The auth frame is native-exact, with an independent Ed25519 over the stamp.
- A provider refusal sends no frame and closes the socket.
- A refused startup read is reported and retried natively, with nothing unsigned sent.
- The private reads sign at release with the provider (independent Ed25519).

`registry/tests/orderly_signer.rs`:
- Prefix normalisation and an independent signature.
- Public-data-only installs nothing.

`tests/history.rs`:
- A provider refusal after an accepted page is strict.
- A transport loss is typed `Network`.
- Funding slots are prepared at their own clocks.

## Size

Not measured locally: the new speed mode skips local combined measurement while headroom exceeds 200K, and this slice changes no engine code. CI's Rust SDK run measures the Orderly family and combined at the exact head.
