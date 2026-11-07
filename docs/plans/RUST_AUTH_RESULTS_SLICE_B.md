# Phase 6b authentication results — native slice B checkpoint

**Landing status (2026-10-21):** slice B and the four earlier WIP hooks
(Crypto.com, Lighter, OKX, WOO) were dropped when the bridge landed; those
venues keep the default `account_auth_outcome` (no evidence) on
`feat/rust-rewrite`. Only the foundation and the tested slice A classifiers
(BingX, MEXC, WEEX) merged. The anchors below describe the dropped drafts on
`origin/feat/rust-bridge-fixes`; resume from them with dedicated tests and an
independent review.

**Incomplete draft, stopped at the user's wind-down request.** This document
records source state for a later explicitly authorized resume. It is not a
claim that venue credential backoff is verified. No automatic follow-up is
scheduled. The core/host/SDK foundation and its tests are separate completed
work; this slice has not received its required venue correctness review.

The approved contract is the refinement at the top of
[RUST_AUTH_RESULTS.md](RUST_AUTH_RESULTS.md). The compiled hook is
`Venue::account_auth_outcome(&self, pending, response, now)`; `now` is HTTP
completion time. Existing pending entries capture the selected signer slot and
private read scope through `Cx::tag_account_read`. The observer must remain
pure, preserve retry admission and session fences, and return `None` when
credential evidence is unknown. Native trading, histories, public metadata and
unsigned account lookups must not manufacture authentication recovery.

## Source checkpoint

All paths below are relative to `sdk-rs/venues/`. The first four draft hooks
(Crypto.com, WOO, Lighter and OKX) were checkpointed before the landing merge;
the remaining implementation and the Crypto/OKX additions remain draft edits.
No new venue authentication tests have been written or executed.

| Venue | Dispatch / observer locations | Draft behavior and missing proof |
| --- | --- | --- |
| ApeX Omni | `apexomni/src/lib.rs:130`, `apexomni/src/account.rs:1428` | Primary private positions/orders/balance; code 10002 rejection; suppress existing first legacy signature fallback. Pending snapshot generation and row validity checked. Needs actual dispatch, fallback and missing-list controls. |
| BreakoutProp | `breakoutprop/src/lib.rs:355`, `breakoutprop/src/lib.rs:1184` | Primary login/discovery/combined portfolio only; no quote/instrument recovery. Existing generation and once-only 401 refresh fences, login 401/403 rejection, local typed session acceptance. Needs real queued retry, stale-generation and public-only controls. |
| Coinbase futures/spot | `coinbase/src/rest.rs:99`, `coinbase/src/account.rs:2044` | Primary JWT account reads; narrow existing 401 rejection, no generic 403/404 rejection. Typed portfolio/product/account payloads. Needs all product probes, partial/malformed snapshots and real signed dispatch tests. |
| Crypto.com | `cryptocom/src/account.rs:307`, `cryptocom/src/account.rs:724` | Primary posted private reads; final 40101/40103 verdicts, existing first signature retry excluded, stream-invalid and missing balance state excluded. Needs real resend/error/missing-list tests. |
| GRVT | `grvt/src/lib.rs:267`, `grvt/src/account.rs:1992` | Primary login/account jobs; captured auth generation must remain current. First 1000 session refresh excluded; final 1000/1001 rejected; valid session headers/summary/streamed flags for acceptance. Needs session/header rotation, shared waiters and documented-code controls. |
| Hyperliquid perpetuals/spot | `hyperliquid/src/account.rs` | Unchanged default `None`: `/info` wallet/account polls are unsigned and prove no API-wallet credential. Add explicit inert integration controls; none added yet. |
| Kraken futures | `kraken/src/account.rs:332`, `kraken/src/account.rs:354`, `kraken/src/account.rs:1414` | Actual selected GET signer captured in signed parts; exact `authenticationError`/`unauthorized` error tokens and native success shape. Needs Secondary/Primary exact-header tests, stream missing/error cases and balance shape controls. |
| Kraken spot | `kraken/src/spot/account.rs:248`, `kraken/src/spot/account.rs:941` | Primary posted balance/orders only; token acquisition/writes excluded. Exact EAPI/permission error tokens; first existing signature resend excluded. Needs all resend/nonce/rate/invalid result controls and real dispatch. |
| Lighter / Robinhood | `lighter/src/account.rs:702`, `lighter/src/account.rs:1929` | Primary token-authenticated active orders only; minimal 20013/21109 rejection. Unsigned lookup/snapshot/limits remain untagged. Existing `account_auth(now)` token generator is untouched; observer is separately named `account_auth_observation`. Needs token-only, market-partition, malformed and unsigned controls. |
| OKX variants | `okx/src/lib.rs:292`, `okx/src/account.rs:149`, `okx/src/account.rs:1435` | Selected private GET signer captured for streamed lists and unstreamed config. Existing account generation fence plus exact 50105/50110/50111/50113/50114/50119/50120 codes. Acceptance still needs the tightening called out below. |
| Paradex | `paradex/src/account.rs:460`, `paradex/src/account.rs:498`, `paradex/src/account.rs:1996` | Primary signed auth acquisition/JWT account reads; exact typed REST error tokens, never WS numeric codes. Auth success excludes responses at/after the existing five-minute admission deadline. Deferred/delegated signing and direct auth route unchanged. Needs token fallback correction and age/queue/signature/typed-error tests. |
| Polymarket | `polymarket/src/account.rs:272`, `polymarket/src/account.rs:684`, `polymarket/src/account.rs:1109` | Primary L2 balance/orders and L1 derivation only; exact HTTP401 error token differs by endpoint. Derivation's existing retry budget suppresses intermediate rejection. Unsigned time/positions excluded. Needs actual derivation retry, supplied credentials, malformed pages and unsigned controls. |
| Txflow | `txflow/src/account.rs` | Unchanged default `None`: unsigned main-wallet account polls do not prove signing credentials. Add explicit inert integration controls; none added yet. |
| WOO | `woo/src/lib.rs:201`, `woo/src/account.rs:1615` | Actual selected GET signer; Info/positions/orders/automatic leverage only, command leverage/history/writes excluded. Exact numeric -1001/-1002, native order generation fence. Acceptance still needs the tightening below and account-page tests. |

## Required work before acceptance

1. Finish the admission audit before adding positive recovery claims. In
   `okx/src/account.rs:1454`, Config currently uses `true`; validate the actual
   nonempty typed Config array/position-mode as the account callback does, and
   require the native balance risk-row shape where appropriate. In
   `woo/src/account.rs:1647`, automatic leverage currently checks only that
   `data` is present; validate its typed leverage fields and the relevant
   Info/page requirements. In `coinbase/src/account.rs:2073`, align accepted
   balance/spot-holding guards with existing native financial validity checks
   (including negative amounts/duplicates), rather than relying on parsing alone.
2. In `paradex/src/account.rs:2042`, the draft token fallback uses
   `jwt_token.or(token)`, while native acceptance first filters an empty
   `jwt_token` before falling back to a valid `token`. Make that predicate
   identical without changing native retry/signing behavior. Recheck account
   balance value selection and malformed typed-error behavior against the
   existing callback.
3. Re-audit every selected signer, scope, stale generation and retry-admission
   branch against current source after landing merge `430fef0874`. Ensure exact
   documented error tokens apply to the particular account endpoint. Primary
   evidence is recorded below; unknown fields/codes/statuses must stay `None`.
4. Add per-classified-crate real Engine dispatch/Notice controls, not only direct
   observer unit tests: accepted validated rows, definitive rejection, public or
   unsigned None, unrelated business/rate None, malformed and missing streamed
   lists, transport/sign failure, duplicate/late consumed request, retry None
   followed by accepted/final-rejected, and actual Primary/Secondary routing.
   Add both inert venues' unsigned controls. Cover Paradex just-before/at/after
   JWT admission age and GRVT/Breakout stale generations/header rotation.
5. Run the complete 13-crate all-feature native suites, strict Clippy, scoped
   formatting and bans; obtain independent source/test review; then coordinate
   a fresh combined WASM build and actual SDK policy/backoff proof with the
   parent. No combined artifact was rebuilt for this draft. Do not reuse a
   foundation-only SDK test as a claim of native endpoint classification.

## Evidence retained from the planning audit

Use these as endpoint-specific evidence, not a universal allowlist:

- [Crypto.com common API errors](https://exchange-developer.crypto.com/exchange/v1/docs/api/websocket-common-api-reference): 40101/40103; preserve existing re-sign admission.
- [GRVT authentication troubleshooting](https://help.grvt.io/en/articles/14117414-how-to-troubleshoot-authentication-errors-when-using-the-grvt-api), plus existing native typed 1000 refresh admission. Reconfirm final permission-code applicability in tests.
- [ApeX Omni errors](https://api-docs.omni.apex.exchange/#errors): 10002 wrong key; 10003 must not be copied into an auth allowlist.
- [Kraken API errors](https://support.kraken.com/en-au/articles/360001491786-api-error-messages) and [Futures authenticated endpoint errors](https://docs.kraken.com/api/docs/futures-api/trading/cancel-all-orders-after): exact error tokens; nonce is not credential rejection.
- [WOO error codes](https://docs.woox.io/#error-codes): -1001/-1002; rate code -1003 is excluded.
- [OKX CEX error codes](https://www.okx.com/docs-v5/en/#error-code): selected 501xx credential/permission codes; timestamp/rate/business codes excluded. Planning extraction: `/tmp/bridge-auth-okx-primary.txt`.
- [Lighter error codes](https://apidocs.lighter.xyz/docs/data-structures-constants-and-errors): minimal token-auth rejection codes. Planning extraction: `/tmp/bridge-auth-lighter-primary.html`.
- [Paradex official REST OpenAPI](https://github.com/tradeparadex/paradex-docs/blob/main/fern/apis/prod_rest/openapi/openapi.json): typed `error` string enum, distinct from WS codes. Planning extraction: `/tmp/bridge-auth-paradex-primary.txt`.
- [Polymarket error codes](https://docs.polymarket.com/resources/error-codes): exact L1/L2 error strings, not substring classification.
- Coinbase's narrow 401 and Breakout's login401/403/once-refresh behavior are existing native and TS endpoint contracts; this draft does not claim a generic all-403 rule or unverified numeric-code mapping.

## Checks at wind-down

- Thirteen selected crates `cargo check --all-features`: **PASS**, 3.51s,
  `/tmp/bridge-auth-slice-b-check-first.log`.
- Scoped source formatting applied with `rustfmt --config skip_children=true`;
  unrelated venue trading files were not formatted.
- Strict 13-crate all-target/all-feature Clippy: **PASS**, exit 0,
  `/tmp/bridge-auth-slice-b-winddown-clippy-final.log`. The first attempt at
  `/tmp/bridge-auth-slice-b-winddown-clippy.log` caught only three new observer
  blocks placed after existing test modules; those blocks were moved before
  the modules without semantic changes. No lint suppression was added.
- Scoped whitespace checks: **PASS**. No full native tests or bans were run
  for this draft during wind-down.
- New per-venue authentication test count: **zero**. Native classification,
  retry/generation behavior and rebuilt-WASM backoff remain unverified.
- No staging, commits, WASM builds or artifact replacement by this lane.
