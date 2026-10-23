# Lighter synchronous history Read terminals

Plan-first owner codex-zs5e83, based on actual `6c7f4fcf292d01b8c056127a070783e3fd72dcd2`.
Branch `feat/rust-lighter-local-history-context`. This plan changes no production.
Read with RUST_REQUEST_CONTEXT.md and RUST_LIGHTER_REQUEST_CONTEXT.md.

## Actual lane count and next slice

The actual checker inventory is173fingerprints/196occurrences:

| Family | Fingerprints | Occurrences |
| --- | ---: | ---: |
| GRVT | 53 | 56 |
| Paradex | 44 | 51 |
| Lighter | 39 | 41 |
| Hyperliquid | 37 | 48 |

Sam's B/C remote-signing decision holds Paradex private account/history/identity/
trading36F/43O. Immediately workable synchronous/public slices total18F/18O:
Lighter local history7, GRVT local history7, Paradex local candle refusals2 and
book admission-refusal1, Lighter public catalog1. The remaining119F/135O have
other concrete holds: GRVT account/lib #1905 and session/history composition;
HL account/lib/history #1982 and signer/write prerequisites; Lighter trading/sign
#1967 and multi-page history geometry; Paradex's remaining candle fallback/Plan
geometry. Those are not attributed to Sam's decision. Only this Lighter local
history slice is authorized in this PR; subsequent areas use separate branches.

Fresh open-PR file lists show no Lighter history.rs claim. #1967 holds its signer/
trading files. No source, tests or helpers from those PRs are touched. The two
largest immediately clear coherent slices tie at seven local terminals; Lighter
is first, then GRVT, per overseer assignment.

## Native branch order and exact move

Within history_query, preserve every validation and credential-presence decision
in its original order. Migrate only these seven immediate terminal branches:

1. Unsupported OrderByClientId fails before other query validation.
2. Unsupported Income category fails before bounds, order index, market or auth.
3. Invalid/nonfinite/negative/reversed bounds fail before order-index/market/auth.
4. Invalid native numeric order index fails before market/auth.
5. Missing requested market fails before private-identity handling.
6. Private history with keyless/public/ineligible native signer state returns[].
7. Otherwise unavailable private identity fails with its exact existing message.

At each existing branch, after deciding that branch but before publishing its
native result, call existing Cx::admit_http_command with Read, credentialNone,
20000ms, RetryPolicy::Never and no pacing. Rejected already emits one typed
entry failure; Revoked denies repeated/outside/already-owned entry. Both return
without a numeric fallback or second result. Successful admission immediately
consumes the same original root using done_context or fail_context, handling the
returned Result without unwrap or a replacement terminal. Keep native error text,
empty output[], and untyped native error kind/outcome exactly when admitted.
No companion notice currently exists and none is introduced.

No Context is issued on remote fallthrough. All remaining history_request,
Request/CursorHistory/Completion, paging/staging, row codecs, auth headers,
query/route bytes and asynchronous failure/finalization source stay unchanged.
No local admission/finalizer wrapper, mixed authority enum, Plan, signer, timer,
queue, retry policy or new shared seam. No lib.rs or account/trading change.

## Inventory

Lighter family39F/41O ->32F/34O. history.rs10F/10O ->3F/3O. Other family/path
maps remain exact incoming. Only remove these seven singleton fingerprints:

- `05267b6f1fa97972a9adb12d43d7cc145356c749dcaf345decbc6b914335d18e`
- `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b`
- `43eef047be9375351e4b6c48ab87b405865d076d1f8b8f01a416086ebe7c6972`
- `79e87d62ec17f7bf661325e15041d94257db847779850b97696a982d07699860`
- `bc8873d3293d9ef3e4011d84e43669c3922f9f9ce78df72ea42a8040419389e5`
- `bde2b4607b65d0c5a80f7cc74527b97cf172bf2f01c6a15af05a3d59cccf9083`
- `be82ef9cf88debeece0fea3eca405694b32c7c83cc5256bc463fb0ec3fed4841`

The retained entries include the physical request and conservative lexical
macro fingerprints; they remain legacy until their separately owned migration.
No bootstrap, rehash, exception or scanner change is needed.

## Original REDs and caller controls

Commit a new dedicated tests/request_context_local_history.rs target first and
run it on unchanged production. Use real Engine dispatch and genuine retained
Read peer roots issued through a test-only delegating venue (same existing
fixture pattern as WEEX local history), not forged contexts or capacity mocks.
With256 retained peers, all seven local branches must give exactly one shared
Internal capacity refusal and no financial outcome, Store publication, I/O or
new timer; after retiring one peer, the refused command ID can complete with its
native result. With NaN/positive/negative infinity clocks, all branches must give
one shared InvalidInput entry failure, then recover at finite time. Test an
already-owned ID without consuming its genuine peer, plus repeated immediate
local completions beyond the owner bound. The capacity/clock controls must fail
as assertion REDs on the unchanged source; no compile failure counts as proof.

Working controls pin exact finite native output/error text and precedence across
public/private state, all four Lighter/Robinhood deployments, income/range/index/
market collisions, empty keyless history and unavailable private identity. Keep
existing tests/history.rs assertions unchanged. A known-market unsigned public
trades fallthrough must retain its exact legacy route/query, headers and one
physical request even when local admission capacity is full; no unused Read root
may be taken for that path. This control does not claim managed remote history.

Caller-visible: successful local replies keep native JSON/errors and signatures;
entry refusal now takes precedence at publication when clock/capacity/custody is
invalid. Every query is still a Read; no NotSent/Unknown financial outcome is
introduced. Architecture gate: No, consuming the existing shared entry/terminal
APIs without venue admission policy or transport logic.

## Reviews and verification

Independent source/adversarial, caller and full-map reviews before production,
repeated on the implementation. Intended files: history.rs, the new dedicated
test target, this plan, README index, CALLERS/AUDITS/QUALITY appendices and the
exact own history manifest prune. All existing source/tests remain byte-exact.
Run original RED target, final focused target, Lighter and Binance native tests,
strict touched-crate Clippy/scoped fmt, bans and both request-context checks.
CI owns WASM/replay/size/timing. Open a separate draft PR into feat/rust-rewrite,
keep one120s CI watcher, then ask for a venue train slot only when green and
reviews clear. Separate exact full-map/revert guard precedes a head-matched merge.
