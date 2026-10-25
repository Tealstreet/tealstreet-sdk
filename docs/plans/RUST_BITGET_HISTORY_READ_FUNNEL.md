# Bitget history Read custody

Owner: codex-ih35js. Base: `8fcedfbbdfc8c59037efda596762802c92e058c3`.
Branch: `feat/rust-bitget-history-read-funnel`. This pin is plan and originals
only; no runtime, shared engine, Cargo, fixture or ratchet changes.

## Scope and ownership

Migrate Fills, OrderHistory, Income and unsigned PublicTrades under their genuine
command Read root. Remove history's last legacy sender and its sign-error
terminal in lib.rs. Account, identity, discovery, trading, sockets, signer recipe,
row financial mapping and existing fixtures stay outside this slice.

Credit claude-4yjlw0's plan and original controls in stale PR #1977 at
`ba3d70b7546`. That owner confirmed no active or unpublished successor, no current
review/test receipts to carry, and this scope. The earlier branch is untouched.
The current source and APIs, plus newly executed originals, are the evidence for
this slice. Send 4y the new PR number so they can close #1977 with a pointer.
Reviewer: claude-fe7lgm (API/native/custody) or claude-dazzy3.

## Exact inventory

On the base Bitget has 15 fingerprints / 16 occurrences. History is 11/12;
lib.rs contributes 3/3 and account.rs contributes 1/1. History is the ONLY caller
of `send`; `signing_identity` still has a trading caller and is retained.
The source cut removes 14/15, leaving the unrelated account site alone.
The repeated range refusal is one fingerprint with two occurrences.

| File | Fingerprint | Before | After |
| --- | --- | ---: | ---: |
| `venues/bitget/src/history.rs` | `23ef96f6a3a6bffcc8ff59464c49bb0586b26c6dfb08d9545a23adefb33ed85e` | 1 | 0 |
| `venues/bitget/src/history.rs` | `0c9d5d0f7d796722ce53df7510cca54a56ef6d223a5ea1a5ba97031eac90cfd5` | 1 | 0 |
| `venues/bitget/src/history.rs` | `32a96a9dd50cf16194031f814e7317103aec0d070636be44a328a7b6ec341de9` | 2 | 0 |
| `venues/bitget/src/history.rs` | `461218ae9b57ef0721cb0c05efdd86bbbad4237d60d3eb56b7ea9d551ccd7469` | 1 | 0 |
| `venues/bitget/src/history.rs` | `b9abde2a6ce07352e15fc0623d9e32e824de326e7b1419a5a7ed81b3a0ce8da4` | 1 | 0 |
| `venues/bitget/src/history.rs` | `5ecc36ad4752a02463dfaef492342fbfe806b4d7a818f3d6e69abdb6bb530cc6` | 1 | 0 |
| `venues/bitget/src/history.rs` | `5659fbaf5a23b68fdd4bbfae6a049a743a01cd5496b4972c6adca2d9e793dcba` | 1 | 0 |
| `venues/bitget/src/history.rs` | `755a658b5cfaa9ec659ee94f62b5f535e94a4a91391476bda23ef1199af846bf` | 1 | 0 |
| `venues/bitget/src/history.rs` | `d5e898e07cc7c2b9f7927c0f9a7f462214e004fdec265ff4f3ed0370a6db1a81` | 1 | 0 |
| `venues/bitget/src/history.rs` | `ea05be25289416d5965f7c723a008695f208a701037575824aea3d1311e9110a` | 1 | 0 |
| `venues/bitget/src/history.rs` | `7ab67768f3a25ca875e762015af8a89f24f2f355f8fd7d26e02fe16e08bdd15d` | 1 | 0 |
| `venues/bitget/src/lib.rs` | `a921be85fe127dcd74b1718af170b7309b86f0b4f66469353f73cc86c722be28` | 1 | 0 |
| `venues/bitget/src/lib.rs` | `6c77d608b079104cc5338cbabb01a7c4a0e24f112ef91ea5483c423808d92bad` | 1 | 0 |
| `venues/bitget/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | 0 |

Delete `send` rather than retaining an unsigned, signed or staging fallback.
The current request checker has no old STAGING_BOOTSTRAPS row to edit; do not
reintroduce the obsolete allowance from #1977. The request map changes only the
rows above. The credential map is unchanged (account 2, identity 1, lib 2
read_credentials mentions). Reuse the existing `read_selection` result; no new
Slots construction or raw-read-key mention. Signing recipe/oracles and the
signing/socket inventories are unchanged. No new primitive use or exemption.

## Admission and authority

1. In the actual Command::Query entry, compute the native local decision using
   immutable state only: private credentials, unsupported client-ID query,
   finite/nonnegative/ordered original range, single-bound completion, ordered
   completed range, then the 1000-window cap. No timer, request, Store mutation,
   progress or terminal occurs during that computation. A local refusal uses
   a keyless root and preserves the native first error/text after admission.
2. For a valid private query, call existing `read_selection` once for a history
   GET path. Its engine Slots selection is Secondary when configured, otherwise
   Primary; the degraded-balance exception cannot match any history endpoint.
   Keep BOTH its opaque identity and Credential for the entire query. PublicTrades
   always uses `(None, None)`, including with valid primary and secondary keys.
   Private eligibility still requires the native ready primary and ignores no
   read-key validation gate. No account-read evidence tag is introduced.
3. `Cx::admit_caller_read_root(credential, None)` admits this genuine entry at
   its original receipt deadline. CommandEntryError::Rejected already emitted
   its terminal. An expired Fresh receipt uses the existing typed entry refusal;
   an unstamped receipt fails closed through the engine, never a default timeout.
   Never replay on_command or manufacture a new command/root after a wait.
4. `ReadCompletion::from_context(cx, command, root.clone())` and
   `ReadHistoryRows<Row>::new(completion, encode)` bind accepted output to that
   root. The Request keeps the original root and unique rows owner, retained
   signing identity/credential and only the native window/cursor/retry state.
   Remove numeric done/fail from every history path and lib sign-error path.
5. At a native eligible dequeue, `Cx::http_leg_context(&root, Policy { Read,
   same credential, timeout_ms: 20_000, pacing: None, retry: Never })` creates one
   physical page. This is always a child of the original root, never a child of
   the previous page. The child is capped by the original receipt. Queue private
   pages via `request_signed_managed` and the unchanged `sign::Http` recipe using
   that retained identity. Queue public pages with `request_managed`; never sign
   the public endpoint. `stream_response` retains the same array path.
6. Before row count, parsing, staging, cursor changes, mode promotion or any
   response effect, require exact live `ReadHistoryRows::validate_delivery` /
   `admit(cx, &leg)` custody. The native page Vec may collect provisional decoded
   rows only inside that DATA; it cannot publish output. After HTTP/envelope/row
   validity, dedup in native order, stage through RowsAdmission and accept_page.
   The seen set changes only with accepted rows. Keep failed-page rollback.
7. Retire the delivered/failed old leg with `finish_http_context` before every
   next page, retry, window or promotion wait. Native timers only establish
   eligibility; they never grant delivery, terminal or selection authority.
   `read_root_retired(owner, pending)` removes only matching parked Requests;
   handed-back in-flight Requests are dropped without Cx effects. Old timers,
   preparation/signing completions and deliveries become inert. Other live
   queries and all background owners survive an expiry/refusal unchanged.
8. Success uses rows.finish_with(Ok, native dedup/sort/serialize). All native
   failure terminals use rows.reject or the admitted root's fail_context before
   rows exist. Never use rows.finish_with(Err) on exhaustion: that API returns an
   accepted prefix, whereas THIS native Bitget query fails strictly. No numeric
   terminal fallback, second terminal or dummy call.

## Native wire and reducer table

| Query | Classic | UTA | Stream |
| --- | --- | --- | --- |
| Fills | /api/v2/mix/order/fills | /api/v3/trade/fills | native envelope lists |
| OrderHistory | /api/v2/mix/order/orders-history | /api/v3/trade/history-orders | native envelope lists |
| Income | /api/v2/mix/account/bill | /api/v3/account/financial-records | UTA data.list; otherwise native envelope lists |
| PublicTrades | /api/v2/mix/market/fills | same, unsigned | native envelope lists |

Keep GET, origin, parameter order and encoding byte-exact. Classic productType
is lower-case, UTA private category upper-case. Limit defaults100 and clamps1..100.
UTA Fills omits symbol, while native filtering remains in the decoder. Keep
orderId, startTime/endTime and cursor/idLessThan spelling/ordering, inclusive
30-day-minus1ms oldest-first windows and the 1000-window cap. The existing native
page counter is global across windows; do not replace it with a per-window cap.

Keep query_item/query_uta_bill financial branches: string IDs beyond2^53,
classic fee sign and separate gross-PnL/fee rows, UTA rebates/ledger amounts,
hedge-close side and position side, inverse market collateral fallback,
unknown income types, category/symbol filters, zero-income filtering, native
seconds versus ms, stable newest-first sorting and first-observation dedup.
No Store/cache publication is added. Existing history.rs/history_fee_asset.rs
assertions remain byte-exact; any unavoidable fixture carry is reviewed separately.

## Native waits, failure and terminal policy

| Trigger | Action | Budget / output |
| --- | --- | --- |
| Private mode not detected | park100ms and re-check native account state | native10s detection deadline; original caller receipt still wins |
| Successful cursor/page or next window | accept page, retire leg, park100ms | same root, cursor/window and accepted rows retained |
| Native response error, missing list/invalid row/cursor or genuine Network/physical Timeout | discard only provisional page, retire leg, park250/500/750ms | three retries then strict `Bitget history incomplete: ...`; no prefix-success or INCOMPLETE marker |
| Native body code40085, including HTTP408/5xx | native promote_uta, clear provisional page and reset detection deadline to now+10s, park100ms | preserve accepted earlier pages/windows/cursor; original root never resets |
| Provider/signing/credential/preparation refusal, malformed stream or typed Internal/integrity failure | retain original typed cause and strictly reject root | no transport retry, no page-success or invented recovery |
| Repeated nonempty cursor or global1000-page cap | native strict error text | no successful prefix terminal |
| Original root equality/expiry, whether in flight or parked | shared engine expiry seals once before venue DATA | matching continuation cleanup only; no timer-created replacement root |

The typed strict local/integrity branch must not be flattened into the native
transport retry, as #2088's correction established. Native response compatibility
uses its actual response path, not an error-message test or a synthetic Network
kind. Genuine physical timeout tests must fire the owned physical timer. Preserve
all typed fields on strict failure. No Mutation/Plan admission occurs here.

## Caller contract disclosures

CALLERS must state the exact original caller-receipt cap over detection, native
waits, all windows, retries, signing and queueing. Root expiry has priority at
equality in either event order. Per-page20s now includes preparation/queue time,
where native transport's default bounded physical time only. PublicTrades gets
managed unsigned custody and the same caller cap. Strict typed integrity/local
failures do not consume transport retries. Native successful mapping and strict
retry exhaustion remain unchanged; accepted prefixes are not silently discarded
on any path that natively returned them (Bitget exhaustion has no such success).

## Originals and source-stage controls

The new target is `venues/bitget/tests/history_read_funnel.rs`. It carries the
older 10 root/compatibility controls credited above, with current enum/handshake
composition and no old execution claims. Ten additive controls pin four exact
Prepare paths, unclaimed response refusal, parked retry equality, Secondary
identity plus public unsigned behavior, HTTP408/500 promotion, later-page strict
exhaustion, and bad-page identity rollback. Every control uses real Engine entry
and real caller wait. Managed paths use actual Prepare/Bind/Claim; originals
intentionally expose the legacy paths without forging Context or ownership.
Record the executed RED/GREEN count on this unchanged source before production.
An unclaimed test intentionally withholds Claim; its bytes must never complete.

The source-stage controls are additive after the frozen original full blob. The
34-test funnel target now passes, including14 added controls for exact endpoint
Primary/Secondary/Public Prepare intents, queue and genuine provider refusal,
held preparation through expiry, fresh signing stamp on Network retry, both root
equality event orders,40085 detection reset without receipt renewal,408 without
40085 at250/500/750ms, typed malformed/UTF-8 refusal with no retry or prefix
success, stale pages,1000-page/repeated-cursor strict exhaustion, and retirement
under actual256-entry registry pressure. Existing native history14/14 and
fee-asset2/2 pass unchanged, covering exact financial and window wire assertions.
The only changes to two newly added controls after their first execution were
pinning the actual malformed-envelope message and isolating history timers from
a native2s account timer collision. No original body or production fix was needed.

## Size, CPU, checks and freeze

This is venue-only; no generic engine source change is authorized by this plan.
Measure paired Bitget and combined on the SAME actual-tip-composed baseline and
first source tree, retain raw/shipped modules, hashes, tool versions, exact heads,
commands and logs, then delete each OWN target. Also measure BitgetUSDC if the
family builder does not prove byte-identical selection. No measurement exists
for this plan. Planning estimate is0..15KiB per Bitget family and combined,
unmeasured; it is neither credited savings nor permission to raise an allowance.
#2158's shrink remains separate. If actual Bitget exceeds its current allowance,
report the exact pair for a budget decision before pushing. When 5c57jl's Bitget
CPU non-regression gate lands, the PR must satisfy it on the composed tree.

Run new target and existing history/fee-asset targets, then the venue native and
strict all-target clippy/fmt checks for source. All bans plus request, credential,
signing and socket ratchets must pass with only this exact own request prune.
On plan/original CLEAR, go to production under standing rules; freeze, one push,
draft PR plus one PR-triggered Rust run parallel with source review. No WIP CI,
job rerun, duplicate dispatch or review/CI carry from #1977. Green exact-head CI
and source CLEAR -> READY to8x5tm1, plain tip merges only, exact-tree GO and guarded
head-matched landing. Then 4y closes the stale PR with the replacement link.

## Executed original receipt

On unchanged production at8fcedfbbdfc, the20-test target ran: **12 assertion REDs,
8 GREEN,0 ignored**. All REDs reached intended assertions, not compile errors:
legacy released HTTP/absent Prepare (four endpoints and managed-release witness),
missing receipt-capped timeout during detection/whole walk/parked retry/isolated
short query, absent20s physical child retry, unclaimed success publication, and
unstamped receipt incorrectly ending NotSent after signing instead of initial
Read refusal with no outcome. The8 controls pin native entry order/text, long
caller wait, retry exhaustion, unsigned public, secondary identity, non20040085
promotion, strict later-page exhaustion and provisional-page identity rollback.
No runtime/map/fixture/Cargo changes were needed to execute the originals.

The unchanged existing native targets also ran on the base: history14/14 and
history_fee_asset2/2 PASS,0 ignored. These preserve exact financial/wire and
fee-collateral assertions; the new target does not replace them.

Scoped strict clippy for the new test target and rustfmt --check PASS. The
original RED log is /tmp/codex-ih35js-bitget-history-originals.log; native controls
and strict checks have separate logs. These are local execution receipts only,
not a source/CI/READY claim.

## Production authorization and strengthened originals

Overseer authorized local production against tip b0ee0a9a4d5 while dazzy3
reviews. Both dazzy3 and fe7lgm cleared plan/originals4207b99c1e9. Before source,
the originals pin the literal unstamped Read refusal (InvalidInput,
"Request context was revoked", no outcome), no Done/Failed or reissued page
from unclaimed bytes, native empty public headers, combined credential-before-
unsupported/range precedence and the whole-walk helper's one-page clock slack.
The native HTTP408/500+40085 promotion remains exercised through real replies;
source adds408 without40085 retry plus typed Internal/provider no-retry.
The CPU rule for this PR is **q <= T**, not the historical absolute bound.

## Local source and measurement receipt

The venue-only cutover removes history11F/12O plus the orphaned lib sender and
sign-error3F/3O. The exact request map leaves the unrelated account1F/1O; credential,
signing and socket maps and generic engine/host sources are unchanged. The
financial decoder body is byte-identical apart from its leading delivery guard.
The reviewed strengthened originals remain an exact full-blob prefix.

Paired baseline on actual b0ee0a9a4d5: Bitget1,468,034 and combined13,908,680
shipped bytes (web and node identical). Method: unchanged harness/build.sh,
Rust1.99.0, wasm-bindgen0.2.129, Binaryen132 required, jobs4, no profile overlay.
Raw/shipped artifacts, SHA256s and logs are retained in the scratch measurement
receipt; each measurement target was deleted. The first-source pair must use
the identical build path and method before any budget or size claim is made.

Focused source execution:34 funnel plus14 existing history and2 fee-asset tests
PASS,0 failed,0 ignored. Full bans and ratchets PASS. Full venue native PASS (333 tests,0 failed,0 ignored). Strict
all-target clippy and scoped rustfmt --check PASS; no CI or READY is claimed.

The orphan sender had two fixture consumers. The secondary-header unit fixture
now enters a genuine Read or serial Mutation Plan and uses managed preparation;
its exact header oracles are unchanged. The history provider-refusal control
uses the real synchronous managed provider and pins the exact typed refusal and
absence of native retry timers. Its orphaned legacy Sign helpers were removed.
All other existing financial, trading, account and socket assertions are intact.
