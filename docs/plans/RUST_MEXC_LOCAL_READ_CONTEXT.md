# MEXC synchronous read command custody

Plan checkpoint, codex-ih35js, 2026-10-24. Base:
`c4e02c38fa474d11547ae017d281ea66748ad02a`. Branch:
`feat/rust-mexc-local-read-context`. No migration is claimed by this plan.
Read with RUST_MEXC_REQUEST_CONTEXT.md and RUST_REQUEST_CONTEXT.md.

## Scope and collision audit

The exact landing manifest has **29 entries / 29 occurrences** for MEXC.
This slice removes **8 entries / 8 occurrences**, leaving **21 / 21**.
Open PR file lists against feat/rust-rewrite were inspected before editing:
no open PR claims MEXC source/tests. AsterDEX #1916 claims account, history,
lib, request, sign and trading source plus relevant tests, so Aster is held.
The other local branch, feat/rust-cryptocom-read-context, stays committed and
parked on the shared auth-health fix; #1917 owns only Phemex history/income.

Only synchronous, pre-I/O MEXC read errors move in this slice. Expected source
paths are venues/mexc/src/history.rs, account.rs and lib.rs (one native local
error projection shared by these two command families), plus a new dedicated
integration test. Shared plan/index/caller/audit/quality docs and the exact
manifest prune accompany implementation. Existing tests/assertions stay intact.

## Exact selected inventory

Each row is one original fingerprint and one occurrence. All eight move to
genuine command-entry Read Context admission followed by fail_context.

| Path under sdk-rs | Fingerprint | Native terminal |
| --- | --- | --- |
| venues/mexc/src/history.rs | `d577b299e0f06807b67ea1998b2b5ef6249fdd97a3dca81b338c1b40978d02a5` | Client-order-ID lookup unsupported |
| venues/mexc/src/history.rs | `def51f5007658efde0e27d55b87c86b7dadd77774069426672bc9fa5c009a8fb` | Income requires explicit FUNDING_FEE |
| venues/mexc/src/history.rs | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | Ordered credential/fills/market/order-ID/time/limit validation error |
| venues/mexc/src/history.rs | `8a87f4b913bf1811f1e4e95b8a7e773e34308f6e57e0140d64dc57fe9eb068b1` | Derived query time range invalid |
| venues/mexc/src/history.rs | `ff15a37413ebcc7d0dac770897169efe8e124ad41528bbde14c92df1ca7ad468` | More than 1000 native windows |
| venues/mexc/src/account.rs | `00f59a2dcd349c524ee87990b60f98bbd090dae7d0e7351b59b6f63a08c9e560` | Account-read pause active |
| venues/mexc/src/account.rs | `4108d1559a4ad73e52be4c21b463683f0720640e6139a19d3dc124fc94a3a924` | Leverage market missing |
| venues/mexc/src/account.rs | `d7aab74134f949f316d653bc421cdba2bd345f74193e4dfc3af0548610e6df25` | Leverage credentials missing |

Unselected inventory remains exact: account.rs 6, history.rs 1, identity.rs 2,
lib.rs 3, secondary_tests.rs 2, trading.rs 7. Account's signed shared request
builders and response terminals, identity, discovery and history's streamed
transport retain their current paths. Conservative test matches remain counted.
Private signer/read-helper adoption stays with claude-4yjlw0 and its shared
prerequisites; this slice consumes no unreviewed signer API.

## Shared authority and native contracts

At each selected terminal, use actual Cx::admit_http_command(Policy), without a
numeric command argument. Declare Read, credential=None, timeout_ms=20000,
pacing=None and RetryPolicy::Never. There is no remote credential use or HTTP
operation on these local errors, including when options contain credentials.
Rejected is already one engine-owned typed terminal; Revoked grants no owner.
Return on either error; do not add a numeric fail fallback or synthetic scope.
On successful issuance, pass the exact native string into fail_context. Preserve
kind=None, outcome=None and all absent venue metadata for native finite errors.
Do not issue a Context on successful continuations: native private selection,
headers/signature, CursorHistory/Completion, paging and account reducers remain
unchanged. Local Context admission settles immediately and adds no HTTP/timer.

History precedence stays: unsupported lookup; unsupported income kind;
credentials; fills need symbol or order ID; known market; numeric order ID;
finite nonnegative ordered times; limit 1..100000; derived time range; 1000-window
tiling. Missing credentials still outranks malformed bounds/limit. Income accepts
FUNDING_FEE case-insensitively. Funding/order-specific/trade reads preserve their
native window exceptions. A from-only range derives its end from floor(cx.now);
the post-derivation check can reject despite valid explicit bounds. General
private history still uses primary credentials independently of account secondary
GET selection. Valid zero-row history is a native remote result, not a new local
empty branch. No decoding, row identity, units, time conversion, accepted-page
recovery, cap or incomplete-warning changes.

Leverage precedence stays: active account pause; known market; primary credential
presence; signed shared request. No secondary-only credential escape or cache-only
success. Pause remains native account pause, with exact existing message and no
new typed rate-limit inference. Valid leverage dispatch retains revisions, both
hedge rails, secondary GET selection and shared failure diagnostics.

Caller-visible difference: these immediate refusals require genuine command
custody. Exhausting the existing 256 managed command owners yields the engine's
typed initial refusal; NaN/infinite command clocks yield InvalidInput. Refusal
does not cancel, replace or settle an existing owner sharing a numeric ID. Normal
finite native errors retain their strings and fields. No SDK argument, result,
Store, signing, account-auth or transport contract changes. Relevant callers are
fetchLeverage/updateSymbolMetadata, client-ID lookup, execution/order history,
funding income/ranges and public trades through Command::Query.

## Test-first commits and review

1. Commit this plan and index before production changes. Independent source,
   caller and exact-map reviews must verify the eight-site boundary and original
   RED controls. Architecture gate: this is native error projection into shared
   command-entry/terminal APIs; it adds no local owner registry, timer, retry,
   credential selection, classifier, paging engine or finalizer policy.
2. Commit tests separately on unchanged c4e02c production. Use real parked MEXC
   managed book commands to fill all 256 owner slots; exercise all eight selected
   branches and require typed initial refusal with all peers still pending.
   Settle a real peer using http_preparation_failed, then retry and require the
   exact native terminal. Cover NaN/+Inf/-Inf clocks, finite command-ID reuse,
   repeated local errors without owner leaks, and duplicate IDs preserving an
   already-owned book. Compilation errors do not count as RED evidence.
3. Native controls cover every grouped history error/precedence, derived-range
   and 1000-window boundaries, real account 429 pause, credentials and absent
   market precedence, and valid signed history/public trades/leverage dispatch.
   Require no local HTTP, ManagedHttp Prepare/Bind, Sign, timer, AccountAuth or
   Store publication. Terminal cancellation cleanup is allowed only where an
   existing managed peer actually settles. Do not weaken old assertions.
4. Implement only selected branches, prune only those eight fingerprints and
   verify scanner equality plus every unowned path against the pinned base.
   Run both check-request-context.py and its 15 test controls. Append actual
   behavior/evidence to CALLERS, AUDITS and QUALITY in the implementation commit.
5. Run scoped MEXC plus Binance native tests, strict all-target Clippy and fmt
   with CARGO_BUILD_JOBS=2. CI owns WASM/replay/size/performance and the repository
   fast lane; no local timing or release build. Independent source/adversarial,
   caller and full-map reviews must be clean before requesting a train slot.
   Push the PR for CI; merge only after explicit overseer GO, fresh integration,
   a separate preservation guard and a head-matched merge.

## Implementation checkpoint

Plan commit `91019396462` received clean independent source/adversarial, caller
and exact-map reviews. Original tests were committed as `d900f2a81fa`, followed
by fixture correction `273327ce332`, both before the production commit. The correction followed production WIP;
its qualifying replay used byte-checked untouched c4e02c production in an
isolated target. Those corrected tests compile and produce **9 assertion
REDs / 6 passing native controls** in an isolated target directory. Eight REDs
individually exercise the eight selected branches at full managed capacity;
the ninth requires typed rejection for nonfinite local command clocks.

The fixture correction keeps the derived from-only bound invalid through slot
recovery and preserves the existing COMMAND_DUPLICATE diagnostic. The initial
duplicate-diagnostic mismatch is excluded from RED evidence. An anomalous archive
run sharing the edited tree's build target is also excluded; the qualifying
receipt uses an isolated target and untouched source. No test is ignored, and
no pre-existing native test/assertion was removed or changed; the new fixture
corrections are recorded separately.

On the adoption, all **15 controls pass**. Focused MEXC/Binance verification
passes **487 tests across 42 targets**, with zero failed/ignored; strict MEXC
all-target Clippy and formatting pass. Both request-context checks pass,
including 15 ratchet controls. The manifest drops only the exact eight selected
entries and occurrences, **29/29 to 21/21**; all unowned maps remain identical.
Production changes are exactly eight local terminal substitutions and one native
error projection calling shared admission/terminal APIs. Successful transport,
signing, paging, reducers and callback bodies stay unchanged. Independent source,
caller and full-map implementation review and exact-head CI are separate gates.

Public SDK leverage reads enter through fetchLeverageForSymbol (native op
fetchLeverage); its existing warning/null catch remains. updateSymbolMetadata
still awaits its command directly. Neither call site changes in this slice.
