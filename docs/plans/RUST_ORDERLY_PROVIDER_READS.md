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
- **Signing at release.** Every signed request's stamp is its release time, the same instant native used whenever no queueing intervened.

## O2 (outline, to be detailed in its own PR)

Each trading command opens `begin_preflight(WriteAfterReads)`, and native's queue parks Preflights in FIFO order. Ordered jobs (place with protections, amend, leverage, margin type) run as serial Plans with each next call taken from the previous call's DATA. Account-wide cancels become complete Plans with `independent_calls`, skipping closed children through `retire_unissued_plan_call`.

Write facts come from a cited Orderly CodeTable. Unknown seals reconcile through `reconcile_uncertain` with a Primary snapshot. Retirement goes through `on_http_owner_retired_preflight` plus `capture_http_reconciliation`.

O2 control (dazzy3): the local provider's advisory `can_sign` reports Ed25519 as unimplemented. Preflight and Plan admission don't consult it today (`Cx::can_sign` is an opt-in multi-release check), and O2 pins with a control that writes are admitted under the installed provider.

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
