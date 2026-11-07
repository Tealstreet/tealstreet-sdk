# Native account-auth slice A — partial wind-down checkpoint

Status: **Partial WIP, not final acceptance and not merge-ready.** The user
requested branch preservation and handoff before the remaining implementation.
Only BingX, MEXC and WEEX were started in this slice. The other ten assigned
crates, their product variants, and actual SDK/WASM end-to-end credential
backoff proof remain unimplemented here. The shared foundation and other
owners' classifiers are tracked separately in [RUST_AUTH_RESULTS.md](RUST_AUTH_RESULTS.md).

## Implemented bounded source

All three use the foundation's pure three-argument
`Venue::account_auth_outcome(pending, response, now)` hook. These ordinary
classifiers ignore event time. Each captures the actual selected signer slot
on the existing request with `Cx::tag_account_read`; no auth ledger, secrets,
serialized DTO changes, retry changes or JavaScript response parsing.

| Crate | Dispatch and observer entrypoints | Scope and evidence |
| --- | --- | --- |
| BingX | `sdk-rs/venues/bingx/src/account.rs:222` (`Account::request`), `:1267` (`auth_scope`), `:1278` (`Bingx::account_auth`); `src/lib.rs:414` trait hook | PositionMode → Settings, Balance, Positions and OpenOrders → their scopes. Saved GET read credential → Secondary, otherwise Primary. Listen-key POST/keepalive excluded. Typed integer codes 100001/100004/100413/100419 reject. Code 0 + HTTP success + typed mode or valid streamed-row flags accepts. |
| MEXC | `sdk-rs/venues/mexc/src/account.rs:88` (`signed`), `:1340` (`auth_scope`), `:1353` (`account_auth`); `src/lib.rs:389` trait hook | Mode → Settings; Assets → Balance; Positions → Positions; regular/plan/stop snapshots → Orders. Capture selected GET read signer. Typed unsuccessful envelope codes 401/402/406/602/701/703 reject. Success true/code 0/HTTP 200 + valid mode 1 or 2, or snapshot data + clean staged rows accepts. Command-only leverage and rare identity queries excluded. |
| WEEX | `sdk-rs/venues/weex/src/lib.rs:232` (`signed_request`), `:516` trait hook; `src/account.rs:778` (`auth_scope`), `:791` (`account_auth`) | Balance, Positions, regular/algo Orders, config/metadata Settings. Capture actual saved GET read signer. Current account-cycle fence applies to both verdicts. Typed integer/string codes 40006/40009/40012/40014/40018 reject. Success requires HTTP success, no invalid streamed rows, and the existing raw-array shape or typed account metadata. Rare identity queries and all other tasks excluded. |

Line numbers describe this checkpoint; prefer the named functions when they
move. Existing request signing, payloads, routes, startup, polling, financial
reducers, command error attribution and identity receipts were preserved.
Core centrally suppresses Accepted when a required streamed list is missing.

Explicit code sources used by the audit:

- [BingX-owned API reference](https://github.com/bingx-api/api-ai-skills/blob/main/skills/agent/api-reference.md): exact auth codes; 109400 and timestamp/rate codes are unrelated.
- [MEXC contract API](https://mexcdevelop.github.io/apidocs/contract_v1_en/): account-read auth/permission codes; 513 timestamp and 510 rate are excluded.
- [WEEX official error reference](https://weexsupport.zendesk.com/hc/en-us/articles/52122274060569-WEEX-API-Common-Error-Codes-and-Solutions): exact credential/signature/passphrase/permission/IP codes. Continue checking protocol-specific evidence when widening the allowlist; generic nonzero codes are not auth evidence.

## Tests and bounded evidence

New files: `sdk-rs/venues/{bingx,mexc,weex}/tests/auth_results.rs`, two tests
per crate, six total. Each starts the real native engine, completes public
startup and obtains a real signed private account request. They verify the
actual selected key header and matching Primary/Secondary notice, exactly-once
response consumption, HTTP-200 rejection envelopes missing the streamed list,
and silent business/rate/malformed/HTML responses. Public startup reads remain
silent. MEXC additionally pins the startup Settings acceptance before its
account snapshots.

These tests do **not** prove every account-read scope, stale-cycle behavior,
command exclusion, every malformed-success shape, every product variant, or
host/SDK policy recovery. Existing venue suites provide regression coverage,
not complete new-auth acceptance. Independent source review and actual WASM
consumer proof are still required. No release or combined artifact was built.

Focused six-test run: PASS, `/tmp/bridge-auth-a-first3-focused-final.log`.
The first trial failed only because the new BingX test expected premium-index
HTTP before the preceding ticker completed; corrected test startup ordering,
with the first log retained at `/tmp/bridge-auth-a-first3-focused.log`.

Full three-crate all-features native tests: **288 PASS**, comprising BingX 109,
MEXC 80 and WEEX 99, across 22 nonempty suites plus three empty doctest suites.
Log: `/tmp/bridge-auth-a-first3-native.log`.

Strict all-targets/all-features Clippy: PASS,
`/tmp/bridge-auth-a-first3-clippy-final.log`. The first Clippy trial found only
helpers located after existing `cfg(test)` modules in BingX and WEEX; moved
those helper blocks before the test modules without behavior changes. Original
log retained at `/tmp/bridge-auth-a-first3-clippy.log`. Full native tests ran
before this mechanical relocation; final Clippy compiled all targets afterward.
Scoped rustfmt check and repository bans: PASS,
`/tmp/bridge-auth-a-first3-fmt.log` and `/tmp/bridge-auth-a-first3-bans.log`.
No full workspace, wasm32, SDK or CI result is claimed by this receipt.

## Remaining ten crates — audited plan only

No auth implementation or tests were written in these crates by slice A.

| Crate / variants | Exact source seam | Required correctness boundary before implementation |
| --- | --- | --- |
| Binance futures/PM/inverse/spot | `src/lib.rs:354` `signed_request`; `src/account.rs` Request, `on_account_item`, `on_account_response` | Capture GET read slot; exclude listen-key/keepalive/signed public metadata. Account vs endpoint scopes. PositionRisk/SymbolConfig need observational malformed-row evidence; do not alter current reducer failure behavior. Generation fences and buffered PM/spot balance/order shapes need separate controls. Typed -2014/-2015/-1022; do not generalize every negative code. |
| Aster v1/v3 | `src/request.rs:180` `dispatch_signed`, `on_signed_response`; `src/account.rs` Request | Unsigned public branch never tagged. Capture selected credential/version. Share actual signature/timestamp/network retry admission, including its legacy fallback, while auth rejection itself uses typed codes. First admitted resend emits no verdict. Preserve v3 signer/nonces. |
| Bybit all categories/Hyro/options | `src/lib.rs:494` `send`, `:293` `resendable`, `:535` `resend`; `src/account.rs` Request | Only private Then::Account, never signed public reads. Read pending category/session/generation. Preserve exact current resend predicate, including legacy text admission, without using messages to classify auth. Typed 10003/10004/10005/10010/33004, only final admitted attempt. |
| Bitget classic/UTA/inverse/USDC | `src/account.rs:128` `signed_headers`, `:175` `account_request`; parent Pending::Account generation | Capture degraded balance/UTA-assets Primary switch at dispatch; current degraded state at completion is wrong. Preserve generation fence and capability-probe/mode-upgrade semantics. Native account reads do not inherit trading-only signature resend. Decide probe scope conservatively so capability fallback cannot strand a failed scope. |
| Orderly aliases | `src/account.rs:222` `request_signed`, `account_response`; `wire::Rest` | Primary-only signer. Narrow typed error metadata needed. Official -1001/-1002 reject; **-1103 is price/tick business error**, not signature. Keep deactivation and required-data checks unchanged. |
| Backpack | `src/account.rs:224` `signed`, `account_response` | Capture actual saved GET read signer. Only documented REST auth evidence, never WS Envelope/WsError. Generic 403/HTML may be WAF/proxy. Existing positions 404 authoritative-empty path is not credential success. Reuse typed successful account/collateral and streamed invalid flags. |
| Bitrue | `src/lib.rs:278` `issue`, `on_response`, `on_request_failed`; `src/account.rs` Request | Exclude Markets' primary signed read, listen maintenance and command lookups. Preserve once-only -1022 resend in buffered AND missing-list paths, and final read retry admission. Snapshot bundled Account vs normal Orders. Do not copy Binance -2017: Bitrue uses it for insufficient balance. |
| BloFin | `src/account.rs:198` `private_read`, `headers`, `account_response` | Capture actual GET read signer; command-only leverage excluded. Exact 152401/152406. Existing body-substring auth-floor counter should become typed equality without changing failure-floor policy. Validate Mode/Margin payloads and pending row-failure flags. |
| KuCoin futures + spot | `src/lib.rs:233` `signed`, `:270` `signed_streamed`; `src/spot/mod.rs::request` | Futures retries **all failed reads** to budget (signature once, others twice): observer must suppress intermediate failures using actual admission. Spot has separate behavior and no equivalent blanket account resend. Exclude private-bullet POST, public reads and primary definitive order lookups. Typed 400003/400004/400005/400006/400007; timestamp/business codes silent. |
| Phemex | `src/lib.rs:146` `request`; `src/account.rs::account_response`, AccountEnvelope | Capture signed GET read slot, exclude unsigned public/primary definitive lookup. Current cycle fence, expected settlement currency, clean staged rows, typed account and streamed positions completion required. Bundled endpoint is Account scope. Documented REST status semantics require typed venue JSON, not arbitrary HTML/proxy errors. Preserve failure/rate floors. |

## Wind-down boundary

No queued follow-up is authorized by this receipt. This is a preserved partial
branch checkpoint; resume only under a new instruction. Remaining source and
test work must be re-audited against the then-current branch, including the
shared foundation and other owners' changes.
