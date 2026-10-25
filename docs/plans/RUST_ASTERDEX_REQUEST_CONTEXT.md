# AsterDEX RequestContext inventory and adoption plan

Planning checkpoint, 2026-10-23; lane `codex-ih35js`, overseer `claude-ddi2il`.
Pinned landing: `7641650e7b1fc54c55593880ac16299171f60a90`.
Branch: `feat/rust-request-context-asterdex`; base/PR target: `feat/rust-rewrite`.
This checkpoint changes documentation only and claims no migrated request.

## Contract and scope

Use the engine lifecycle from [RUST_REQUEST_CONTEXT.md](RUST_REQUEST_CONTEXT.md),
the managed Binance reference, and the read-delivery validator landed in #1792.
Keep the venue's native codecs and wire facts; the engine owns release, deadline,
retry eligibility, admission and aggregate outcome. No new venue lifecycle,
method-based permission, retry executor, clock mirror or numeric completion bypass.
All 71 lexical occurrences below must disappear before master; none is exempt.

The exact manifest has 47 distinct path/fingerprint entries: account 2/2,
history 11/20, lib 4/9, request 23/33 and trading 7/7 (entries/occurrences).
There are 41 direct legacy API calls and 30 conservative field/macro occurrences;
the latter are not additional physical requests. No Aster `sign_http` occurrence
exists: `request.rs` currently signs synchronously through `Query::signed`, which
still requires the host-signer migration. A zero `sign_http` count does not prove
managed signing or secret removal.

## File ownership checked before editing

All 21 open PR file lists against landing were inspected at this pin.

| File | Open PRs | Consequence |
| --- | --- | --- |
| `sdk-rs/venues/asterdex/src/account.rs`, `tests/account.rs` | #1785, #1786, #1787, #1788 | Wait for incoming row/freshness adoption; do not edit either file. |
| `sdk-rs/venues/asterdex/src/lib.rs`, `tests/trading.rs` | #1799 | Wait for capability work; no public-command integration or trading-driver edits yet. |
| Other Aster source/tests | None in inspected lists | May work here only if the complete change and existing fixtures avoid the claimed files. |
| Shared inventory, CALLERS, audit/quality and docs index | Several | Overseer explicitly permits these shared files: append docs; prune only this venue. |

Recheck open PR lists before every source slice. Aster's current account and
trading fixture drivers consume only legacy `Action::Http`; changing common
startup dispatch requires their preparation adoption too, so the apparently free
`request.rs` is not sufficient reason to change every public read immediately.
Report collisions; never weaken a fixture or send legacy HTTP as a workaround.
On permitted shared-file conflicts keep incoming docs first and our append once;
keep incoming inventory and reapply only this venue's proven removals.

## Engine paths used in the inventory

| Code | Destination | Caller-visible boundary |
| --- | --- | --- |
| R | Engine-issued command read Context, `request_managed`/Lifecycle, existing stream attachment, `done_context`/`fail_context` | Same native query/result; typed preparation/timeout failures, stale/duplicate response suppression and preparation-required custom transport. |
| B | Engine-issued background read Context and managed Lifecycle; retire via `finish_http_context` before native terminal callback | No command completion; retain native startup/reconciliation policy and explicit failure visibility. |
| S | Opaque credential slot/ref plus unsigned `SignStep` and `request_signed_managed`; engine validates fresh release | Preserve V1/V3/Code canonical bytes, primary/secondary identity and routing; no raw credential recipe retained in the venue. |
| H | One shared `HistoryRows`/range owner and Completion across pages, with engine-issued read root/child contexts | Preserve normalized history shapes and strict native proofs; no successful plausible partial financial history. |
| W | Command root and bounded child legs, native `WriteResult` facts, engine journal and Context-bearing completion | Unsent is NotSent, released unresolved is Unknown, known partial effects/IDs survive; never infer outcomes from the first error. |
| X | Shared session-token/WS-auth ownership, then signed managed lifecycle under that shared authority | Listen-key creation/keepalive are background mutations, never semantic reads or fabricated user commands. Existing generation/keepalive recovery remains required. |
| F | Semantics-preserving local binding/fixture field renames, with identical assertions and unchanged scanner | No caller change. Remove conservative matches rather than relax the ratchet or claim they are managed HTTP. |

R/B/S use `Cx.http_context`, `Policy` with explicit `Operation`, credential and
retry semantics, and the original raw deadline. Child work uses the shared
parent-bound issuer. Streaming attaches only after queue success; queue refusal
returns its continuation and settles/retains exactly the original owner. Never
turn a stale Context into a new numeric completion authority. Native Store/cache
freshness and row admission remain additional requirements, not consequences of
having a Context. `Lifecycle` is engine-owned; no venue constructs a second one.

## Ordered, separately reviewable slices

1. **Plan checkpoint (this commit).** Freeze every fingerprint/count below,
   verify it against actual source with the unmodified scanner, document ownership
   and engine prerequisites, and report the commit to the overseer.
2. **First implementation: single unsigned candle command.** After #1799 releases
   `lib.rs`, add original-RED tests in a new `tests/request_context.rs`, commit
   those tests separately, then migrate only `FetchCandles` and its streamed
   response/failure finalizers in `lib.rs`/`request.rs`. Retain an issued read root
   in its pending continuation from ingress to completion; use the same URL,
   interval conversion, native row decoder, result schema and seconds contract.
   Reject unsupported intervals through Context-bearing completion without I/O.
   Adopt `tests/public.rs`'s existing candle harness with the shared prepared-action
   executor; its current collector accepts only `Action::Http`. Preserve every
   existing candle assertion. Add typed failure delivery for this managed pending
   branch: the inherited `Venue::on_sign_error` otherwise reduces `VenueError` to
   text before `on_request_failed`. Do not reclassify that text in the venue.
   The current public wrapper retries transport failures and retryable 429/5xx
   up to three times with 1/2/3-second nominal backoff, clears staged rows and
   rejects negative native error envelopes. Retain those native facts/bounds using
   shared retry eligibility, original-deadline authority and shared backoff. If the
   engine lacks a required policy seam, request it from the owner before moving
   this path. Never silently drop retries or retain a second venue retry policy.
   The original managed raw bound caps the whole read across retries rather than
   restarting on a resend; record the resulting timeout change in CALLERS.
   Avoid switching common startup paths while their fixture files are claimed.
   Prune only fingerprints actually removed; shared hashes with other live calls
   remain at their exact remaining count. A following small PR handles the
   single public depth command and immediate cached command completions.
3. **Single background reads.** Migrate individual discovery/ticker/OI requests
   after all affected fixture drivers can use the existing engine prepared-action
   test executor. Preserve native failure/backoff/initial-load state. Keep the
   clock's multi-attempt fallback and signed preflight outside this first slice.
4. **Single signed reads and writes.** Coordinate host signing before touching
   inline signing: V1 HMAC owner `codex-5c57jl`; shared signing/plan APIs
   `codex-uufub4`. Then migrate identity, STP read, individual settings and simple
   writes through S/R/W with the existing native error/ACK codec and read-only
   late reconciliation. Account work waits for #1785-#1788. Shared row admission
   and snapshot fences land first and remain intact.
5. **History and multi-request commands.** Obtain shared plan admission from
   `codex-uufub4` before batches, brackets, split orders, cancel/replace, paired
   reads, bounded fill/income windows or clock-preflight-plus-action. One logical
   root owns pages/stages and original deadline. History owner adoption alone
   does not make legacy HTTP managed. At this pin `history::Completion` still
   completes numerically and lacks Context authority: request the shared engine
   extension from its owner before claiming H complete; never copy a finalizer.
   Retain seven-day native windows, dedup-before-PnL normalization, delayed income
   pages, saturation errors and no partial financial result. Joined/coalesced
   completion (including shared clock waiters and account repair owners) remains
   last, pending shared ownership support; report evidence, add no exemptions.
   Use the shared strict rejection path for failed financial history: the generic
   `HistoryRows.finish_with` accepted-row recovery is not Aster's strict contract.
   The same signed dispatcher carries listen-key POST (`account.rs:420`) and PUT
   keepalive (`account.rs:981`). Those are background mutations: B's read policy
   and W's command authority cannot represent them. X requires the shared socket/
   session owner (`codex-5c57jl`) and any engine admission extension; current
   background-mutation refusal stays enforced until that capability lands. Never
   copy the legacy `is_read` listen-key exception into managed semantics. This
   hard dependency must be reported before claiming an Aster-zero PR is ready.
6. **Zero and proof.** Rename only incidental `request` bindings and fixture
   `done`/`fail` names that trigger conservative matching, preserving assertions.
   Delete replaced venue retry/outcome machinery only when all consumers have
   moved. Remove every remaining Aster manifest entry, with both ratchet checks
   passing and every other venue map unchanged.

Every implementation slice is a separate small PR with a committed plan update,
a separately committed failing regression before the move, exact venue-only
pruning, and a CALLERS section describing landed behavior rather than progress.
This docs checkpoint has no landed behavior to backfill in CALLERS.

## Regression and verification plan

The first candle controls park preparation (no early HTTP), expire the original
raw deadline, deliver duplicate/late responses, reuse a numeric command ID, refuse
route preparation, and validate streamed candles through the real final claim.
Also cover retryable HTTP/transport failure, exhausted retries, discard of a
failed attempt's staged prefix, native error envelopes, and expiry during retry
delay. No retry may create a fresh root, extend its deadline or leak staged rows.
Preserve every existing endpoint, timestamp, row and command-result assertion.
Assert one terminal completion and no publication from stale delivery. A missing
prepared custom transport must produce the engine's typed failure, never fallback.

Signed controls additionally park signing, revoke/change credentials before
release, prove read identity/egress and fresh V1/V3/Code bytes, and preserve prior
released-write uncertainty after later pre-send failure. Native batch controls
cover accepted/refused/unknown permutations, partial IDs, idempotent ordinary
cancellation and conservative amend cancellation. No automatic uncertain-write
resend, invented IDs or optimistic unproven gateway ACK patches.

Local code gates use `CARGO_BUILD_JOBS=2`, `cargo test -p safe-cex-venue-asterdex`
plus Binance, and scoped fmt/strict all-target Clippy. Run
`python3 sdk-rs/scripts/check-request-context.test.py` and
`python3 sdk-rs/scripts/check-request-context.py` for every prune; compare all
unowned venue maps. Corepack commands use installed NVM Node22, never Volta or
extra global tooling. CI owns rebuilt WASM/replay/size/timing gates; run no local
release build or replay sweep. Start a background `gh pr checks <n> --watch`
when waiting on CI. Seek a merge window only after green CI, complete proof and
clean independent source/adversarial, caller and full-map reviews.

Architecture-gate answer: **No**. This plan adopts shared engine policy and keeps
only native codecs/semantic declarations in the venue. Any missing shared
authority, retry, signing, pagination or plan-admission seam is requested from
its owning lane before adapter implementation; no per-venue replacement.

## Exact pinned inventory

Checkpoint verification: worktree setup completed with installed NVM Node22;
unmodified request-context ratchet and all 13 scanner controls pass. Independent
source/adversarial, caller and full-map reviews are clean after adding the candle
fixture/typed-failure/retry prerequisites and explicit session-token dependency.
Each review answers the architecture gate No. No production source, test,
inventory or caller behavior is changed by this planning commit.

The following tables are mechanically extracted using the unmodified scanner's
fingerprint tokenization. Counts include its conservative field/macro matches.
Each row names its destination above; none authorizes an inventory expansion.

### `venues/asterdex/src/account.rs`

| Full fingerprint | Count | Existing expression | Destination |
| --- | --- | --- | --- |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | `cx . done ( command , data )` | R/S (identity) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | `cx . fail ( command , error )` | R/S (identity) |

### `venues/asterdex/src/history.rs`

| Full fingerprint | Count | Existing expression | Destination |
| --- | --- | --- | --- |
| `00e86f5d65c7a11874e2adf9c5e2ff0823ed9665f8956ff6e8c3fba14d3f6182` | 1 | `cx . fail ( command , "Aster does not support caller client order IDs" )` | H + R/S (or owned no-I/O validation) |
| `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` | 3 | `cx . done ( command , "[]" . into ( ) )` | H + R/S (or owned no-I/O validation) |
| `497d185372b077e0511e2f21fa2b9b14522eab0cd1c71ac0f7a65741155d0a23` | 1 | `cx . fail ( command , error . to_string ( ) )` | H + R/S (or owned no-I/O validation) |
| `749f38f072e426db18da4e713728caa09605c4bae61b15031812ca4046d6e98d` | 1 | `cx . fail ( command , "Aster fills range exceeded request budget; no partial result returned" , )` | H + R/S (or owned no-I/O validation) |
| `99dc418d7a4be445d25b3f090a9e64b85bffe6ec652ba8dd748b583186d68b22` | 2 | `cx . fail ( command , "Invalid Aster history time range" )` | H + R/S (or owned no-I/O validation) |
| `a19da57fec7e9d57bdc56e98b06771ef616eafff44b0877d51d9ee368a705b46` | 1 | `cx . fail ( command , "Aster fills timestamp saturated; no partial result returned" , )` | H + R/S (or owned no-I/O validation) |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 3 | `cx . done ( command , data )` | H + R/S (or owned no-I/O validation) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 4 | `cx . fail ( command , error )` | H + R/S (or owned no-I/O validation) |
| `e8d6a2c8099e6770fc84d54bd84b94100329548dc8607e033d6146341a39895f` | 2 | `cx . fail ( command , "Malformed Aster history response" )` | H + R/S (or owned no-I/O validation) |
| `ef2650178fae03a9600bb28a3783c7366361657cbff6bd24a137587b1d558c9b` | 1 | `cx . fail ( command , "Aster order history requires a time window of at most seven days" , )` | H + R/S (or owned no-I/O validation) |
| `f7650263254ea5581dc41dc88911754ba35b853d80ab2f1340c5261d8608825d` | 1 | `cx . fail ( command , "Aster fills range could not be exhausted; no partial result returned" , )` | H + R/S (or owned no-I/O validation) |

### `venues/asterdex/src/lib.rs`

| Full fingerprint | Count | Existing expression | Destination |
| --- | --- | --- | --- |
| `0c84f26574bbfb5bb1c4dce587925cebfaad0e356b6245aebd577a633da18c30` | 1 | `cx . fail ( command , interval . unsupported ( "AsterDEX" ) )` | R (depth/candles/cached read; includes no-I/O validation) |
| `569abc443c07aca5127ee71c347600be68bb35c14aeae80c3a14b2f5f2642980` | 1 | `cx . fail ( command , error . to_owned ( ) )` | R (depth/candles/cached read; includes no-I/O validation) |
| `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 3 | `cx . done ( command , data )` | R (depth/candles/cached read; includes no-I/O validation) |
| `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 4 | `cx . fail ( command , error )` | R (depth/candles/cached read; includes no-I/O validation) |

### `venues/asterdex/src/request.rs`

| Full fingerprint | Count | Existing expression | Destination |
| --- | --- | --- | --- |
| `06cc18dcddb9168d297e6d379234075ef72cf4bc6141af4f67fae5cefcfed48a` | 2 | `assert ! ( request . headers . is_empty ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `12f490d6bf534417cc659fadb85d7dccb746f1acce5edd7bac9bf4dc6db0af15` | 1 | `h . fail ( clock )` | F (preserve assertion/logic; rename incidental bindings) |
| `283e4b219f3a27dab408c719d64b98f03153539c6be33c1ac7333e9162adf1c0` | 1 | `assert ! ( ! request ( Method :: Delete , "/fapi/v3/order" ) . is_read ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `2e908d557ef24aa75d796f94a1bc279fc2f16a87ff26abd28b5353594331da01` | 1 | `h . fail ( request )` | F (preserve assertion/logic; rename incidental bindings) |
| `2f1259900ccc2e242405788f4092003c6c759ecfc924e723e3b0a760cce32ee2` | 1 | `assert ! ( ! request ( Method :: Post , "/fapi/v3/order" ) . is_read ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `3d18640fb5163d543856f73b63d914fb57f485b22bdff1238b2a6258a38b477b` | 1 | `assert ! ( request ( method , "/fapi/v3/listenKey" ) . is_read ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `62abd564684b5b7a37d3ab5857b7821dc0313083590a954182d36a279a45e22d` | 1 | `cx . request ( Method :: Get , url , Vec :: new ( ) , None , Pending :: Signed ( request ) )` | R/B (unsigned) |
| `839e82acc52aa01a5ce1c76a582a9b6b4976082960b8c35c3335a05598552c1b` | 1 | `matches ! ( & request . pending , Pending :: ExchangeInfo { . . } \| Pending :: Daily { . . } )` | F (preserve assertion/logic; rename incidental bindings) |
| `88bdec8ef506ecbd68261b35ee606556be07fe3c55808a6207547bfa573be0e0` | 1 | `cx . request_streamed ( method , url , headers , at , Pending :: Signed ( request ) )` | S + R/B/W/X |
| `963a7509aa2658d9ea3fe731b6517baad6a71f0e96dcf6310adac3cc8d57078a` | 2 | `self . done` | F (preserve assertion/logic; rename incidental bindings) |
| `a672a578fda69c95c5b016d238bdf330892623a317a07c7efdb3b610655836b0` | 1 | `format ! ( "{}{}" , self . urls . rest , request . path )` | F (preserve assertion/logic; rename incidental bindings) |
| `aab29994042d311bf1fc36ab564ac02200511c9b6510b4113f2cea4930bfbf30` | 5 | `h . done` | F (preserve assertion/logic; rename incidental bindings) |
| `ab7a36dd0d4c9264d6d2d56fa06128c1e3c698f0021edc1a95f6219b668377a7` | 1 | `cx . request ( Method :: Get , format ! ( "{}{}/time" , self . urls . rest , version . prefix ( ) ) , Vec :: new ( ) , None , Pending :: Clock ( request ) , )` | B (clock; shared fallback plan later) |
| `ac0ab51af525155c29ac31d566180fdc5c6786017018a638795d64750595622f` | 1 | `assert ! ( request . body . is_none ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `ad7a508147ab0edf650d46bad3cc6cb526160e82895f642d08eed8aa02ff7c95` | 4 | `assert_eq ! ( h . done . len ( ) , 1 )` | F (preserve assertion/logic; rename incidental bindings) |
| `bafec4f858a24b4270a4cd2171af0a0e1c4718547d39a67e9d3087a3fe85fdaf` | 1 | `assert ! ( ! request . url . contains ( "signature" ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `c7499fb4f4dd5797ece588f4c3390b704f05afab44def30aad974bf9ef21da31` | 1 | `assert ! ( h . done [ 0 ] . is_err ( ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `ccddcfe32fa0f5a9523ce204e4ce74b5e9458fdb66c502bbca3788b36ab80c0f` | 2 | `assert_eq ! ( request . timeout_ms , None )` | F (preserve assertion/logic; rename incidental bindings) |
| `ed098c61aaf8a45d8cdcb36f8e51d2b326687acae311b2b65c50be4c5d5ba8be` | 1 | `format ! ( "{}{}?{query}" , self . urls . rest , request . path )` | F (preserve assertion/logic; rename incidental bindings) |
| `f7818fa1b23d5e723b91adaaf8f12a2dbd27f9bd93c7d4f70190205b1437fd96` | 1 | `cx . request ( method , url , headers , None , Pending :: Signed ( request ) )` | S + R/B/W/X |
| `fb99b1c1d6d86022bfc6f4d49cb3203922f172a89f92537eba96f11c6a9b6a07` | 1 | `assert_eq ! ( request . headers , vec ! [ ( "X-MBX-APIKEY" , expected . into ( ) ) ] )` | F (preserve assertion/logic; rename incidental bindings) |
| `fc545f085b945207fecfb5193f5e012b87aa0707f358a613848c65808fad9b45` | 1 | `assert ! ( request . url . contains ( "timestamp=1700000012345" ) )` | F (preserve assertion/logic; rename incidental bindings) |
| `fcaa35ab906ea576e79d8a21116684b65e70d1685169771073019029221396d5` | 1 | `cx . request_streamed ( Method :: Get , url , Vec :: new ( ) , at , Pending :: Signed ( request ) )` | R/B (unsigned) |

### `venues/asterdex/src/trading.rs`

| Full fingerprint | Count | Existing expression | Destination |
| --- | --- | --- | --- |
| `2f0770d2ea5f5246cd2ddcd35b614bce738e564c2d95193ff5a84eb8479811e5` | 1 | `cx . fail ( id , not_sent ( error ) )` | W (STP read uses R/S) |
| `488da2b855444a6285294159c398a6aa8dc7f130410e9940370a66662c3e7eef` | 1 | `cx . done ( id , job . result . unwrap_or_else ( \| \| serde_json :: to_string ( & job . ids ) . unwrap_or_default ( ) ) , )` | W (STP read uses R/S) |
| `a41063f04016af862c5272c6ee36a28f313b9e5dbef3294b7c65d74c86df29d0` | 1 | `matches ! ( & request . call , Call :: Cancel { filled_at_amend : None , . . } )` | F (rename incidental binding) |
| `b57c0e274933eb087e87e945d41b0c3fb51bb9e53ab1d343454facd2109f5f69` | 1 | `matches ! ( & request . call , Call :: Place { . . } )` | F (rename incidental binding) |
| `e2f7439f01b028bfc35d2a2433f63a73448c83dad981d0d281f24deb4059167b` | 1 | `cx . fail ( id , not_sent ( error ) . kind ( ErrorKind :: AuthInvalid ) )` | W (STP read uses R/S) |
| `ea0ff1d0bf1689d0410b0bcc1cbbeedc2fa1bec60349440b50760648369edf0e` | 1 | `cx . fail ( id , error )` | W (STP read uses R/S) |
| `fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30` | 1 | `cx . fail ( id , not_sent ( error ) . kind ( ErrorKind :: InvalidInput ) )` | W (STP read uses R/S) |

## Owning-lane coordination after the inventory checkpoint

`codex-5c57jl` confirms Aster is outside its eleven-family HMAC list: it owns
no Aster signer source edits. Aster V1 and V3/Code provider adoption therefore
needs an explicit owner assignment from the overseer before signed source
changes. The native codec/preimage distinctions remain required. Shared
background-session Context authority belongs to `codex-uufub4` (Binance/BingX
first); Aster socket adoption follows the shared core and its fixed size budget.
This supersedes the provisional owner routing in steps 4/5; no assignment or
shared capability is implied by sending the dependency request.

`codex-uufub4` confirms its initial Plan core provides declared physical attempt
counts, shared/fresh-attempt windows and inter-step waits, but does not yet
provide the engine-owned same-call retry wakeup seam. That early Plan PR alone
does not unblock the first candle slice. The exact current read-retry contract
sent to the owner is three retries after the first attempt, with delays exactly
1000, 2000 and 3000 ms (plain `set_timer`, no jitter), for transport failures and
the existing GET 429/5xx policy. `read_retries_three_times_with_linear_delays_and_fresh_signatures`
asserts those exact delays. Reset clears staged Klines rows/invalid state;
negative native error envelopes remain terminal. Candle-specific partial-stream,
retry and immutable-deadline regressions still need their original-RED commit.

Overseer assignment following those requests: Aster V1/V3 signer adoption is
owned by `codex-sy8pn5` (after ApeX); listen-key adoption is owned by
`codex-spc1lj`; the same-call retry wakeup requirement remains with
`codex-uufub4`. Detailed codec, credential-routing, session-generation and exact
retry-delay requirements were sent to those owners through Aimux. These named
assignments supersede the provisional routing above; source adoption and shared
API availability remain unclaimed.

Overseer lane-ID correction: the original signer/session Codex lanes died at
startup. Their replacements are `claude-19pt8s` for non-HMAC signing, including
ApeX then Aster V1/V3, and `claude-55ao7t` for background-session Context. The
codec/routing and listen-key requirements were resent to these replacement
owners through Aimux; the earlier messages to the dead IDs did not establish
delivery. Shared error-classification ownership is now `claude-dazzy3`; this
checkpoint has no outstanding request to its predecessor. Retry scheduling
remains with `codex-uufub4`.

## Remaining funnel checkpoint (2026-10-25, claude-19pt8s)

Base `c6a5c4bb0a4`; branch `feat/rust-asterdex-managed-funnel`. Docs only.
Earlier slices removed 39 of the 71 occurrences (local read admission, local
trade refusals, the listenKey session write). The unmodified scanner now
reports **23 fingerprints / 32 occurrences**:

| File | Sites (line at base) | What it is |
| --- | --- | --- |
| `request.rs` | 207 `format!` over `request.path`; 210/213 public `request_streamed`/`request`; 256/257 signed `request_streamed`/`request`; 351 `matches!(&request.pending, ..)`; 468 clock `request` | The shared transport: every public read, every signed read and write, and the clock. |
| `lib.rs` | 811/812 DepthRead `done`/`fail`; 892/893 Klines `done`/`fail`; 1003 DepthRead `fail`; 1013 Klines `fail(.. to_owned())` | Public command terminals. |
| `account.rs` | 736/737 Info `done`/`fail` | FetchAccount terminal. |
| `history.rs` | 660-891: 13 `done`/`fail` | Fills, Orders, Income and Trades terminals, including the strict "no partial result" refusals. |
| `trading.rs` | 473/475 job `fail`/`done`; 489 and 501 `matches!(&request.call, ..)` | Multi-step trade jobs. |

Every site is accounted for; no Aster `sign_http` remains, and the listenKey
writes already use `session_context` plus `request_signed_managed`.

### Design

Follow the ApeX managed-read unit (#2066) and trade plans (#1891); no new
engine seam is needed.

- **One per-read custody wrapper** (`read.rs`, as ApeX): `Custody::{Public,
  Admitted, Leg}` carries the Context with the unsigned request, so a resend
  rebuilds and re-signs at release. Public reads use `http_context` with
  credential `None`; signed reads select through `SelectedRead` and release
  through `request_signed_managed` with the existing `Recipe`.
- **Background reads** (exchangeInfo, daily, open interest, brackets, account
  Mode/Balance/Positions/Orders snapshots) use `Scope::Background` and
  `RetryPolicy::ReadFailure`, and retire through `finish_http_context` before
  their native callback (`startup.failed`, `seed_daily`, `finish_interest`, the
  snapshot owners). Native startup and backoff state is unchanged.
- **Command reads** (DepthRead, Klines, Info, history) admit at command ingress
  and end only through `done_context`/`fail_context`. History keeps one
  `ReadRoot` per command with a leg per page, retires each leg before minting the
  next, and stages rows through `ReadHistoryRows`, keeping the strict
  no-partial-result refusals and seven-day windows.
- **Clock.** Each `/time` attempt is its own Background read context; attempts
  2-3 use `http_context_with_egress(.., Egress::PublicCors)`, replacing
  `route_request`. The 5 s per-attempt bound becomes the Context deadline.
- **Clock gating.** The command root is admitted at ingress with its
  deadline. `awaiting_clock` holds the unsigned request; its signed leg is
  staged and prepared only after the clock settles, bounded by the root's
  remaining time, because dispatch signing stamps from `protocol_now` at
  prepare. The -1021 resend likewise re-prepares after its resync. The gate
  stays venue-local while Aster is its only user; a second family makes it a
  shared release gate.
- **Trade jobs become managed Plans** (W). Each `Planned` step is a plan leg
  (`plan_leg_context_with_intent`); the job ends through
  `done_plan`/`fail_plan`. `continue_on_error` and `successes` keep their native
  meaning, and released 408/5xx stays Unknown through the shared classifier.
  Writes never resend on network or gateway failure, as today. They keep
  today's two single resends: V1 -1022 once, and -1021 once after a clock
  resync. Each resend re-prepares the same plan leg (attempt 2) with a fresh
  timestamp and signature and frozen economic fields. -1021/-1022 are
  venue-definitive pre-execution rejections, never Unknown. RED tests pin
  both for POST, PUT and DELETE.
- **Resends.** Network retries and retryable 429/5xx on reads use
  `AdmittedRead::retry(ReadFailure)` or a fresh leg, and the shared backoff
  replaces the venue's 1/2/3 s timer (`RequestState.retries` is deleted). V1
  signature rejection (-1022) keeps exactly one resend; the -1021 timestamp
  rejection keeps one clock resync then one resend, both through the same
  Context, never a fresh root.
- **Lexical sites** at 207, 351, 489 and 501 go away by renaming bindings
  (`request` to `unsigned`/`leg`), with identical behaviour.

### Caller-visible changes (for CALLERS when each slice lands)

- Signed and public reads gain an admission-to-expiry bound (20 s, as ApeX),
  starting after any clock wait. Today a read has no venue deadline beyond the
  host default.
- Read retry delays follow the shared ReadFailure backoff instead of the fixed
  1/2/3 s.
- Typed transport and preparation failures reach command errors instead of
  being flattened to text.

### Slices (each a separate PR with RED tests first and an exact prune)

1. **Public reads and clock**: `request.rs` 207/210/213/468, `lib.rs` DepthRead
   and Klines terminals, startup discovery, ticker, OI and brackets. Fixture
   drivers move to `test_support::prepared_actions`.
2. **Signed reads**: account Info and snapshots, all history, the signed GET
   branch of 256/257, and the 351 rename.
3. **Trade plans**: `trading.rs` jobs, the remaining 256/257 write branch, and
   the 489/501 renames. This deletes `Pending::Signed`'s legacy terminal path and
   `RequestState.retries`. Aster reaches zero.

### Budget and risks

The Aster family measures 1,322,746 bytes at base (1,324,108 with #1990)
against 1,343,488, so about 19.4 K remains after #1990. Each slice measures the
family and combined wasm on its composed tree, and deleting the venue retry and
`Pending::Signed` machinery offsets part of the cost. If a slice cannot fit, it
stops and reports; nothing is trimmed silently.

Decisions (overseer, 2026-10-25):

1. The signed release is staged and prepared only after the clock settles, so
   clock attempts never consume the signed leg's own deadline (which starts at
   prepare) and nothing signs while awaiting the clock. The root's deadline,
   from ingress, still runs during the clock wait. The command root is admitted at ingress; the clock
   phase keeps its own bounded attempts.
2. Slice 3 waits for #2093 (`sign.rs`) and merges (never rebases) whichever
   lands first; slices 1 and 2 proceed now. Reviewer:
   claude-m26ps4.
