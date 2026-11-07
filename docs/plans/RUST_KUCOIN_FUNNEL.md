# KuCoin request and dispatch-signing funnel adoption

Owner: codex-zs5e83. Native/funnel reviewer: codex-9czdjh or codex-uufub4.
Assigned baseline: bba295b56824c8bb9a9e727240851b557968c1df. Actual planning
composition: 74db4cd0768e0e03c6834bf3eddb1cb9c3d656e6; the intervening landing
is Kraken socket-plan documentation only. Landing branch: origin/feat/rust-rewrite.
GRVT G3/G6 adoption and Hyperliquid nonce/scalar/pacing adoption resume first
when their prerequisites land. This is the third lane, PLAN FIRST.

## Ownership and preserved predecessor

The overseer assigns inactive draft #2033 to this owner, using the #1954 adoption
precedent: a replacement PR credits the original work and preserves its authorship
through a plain merge. The preservation pin is the PUBLISHED head
c87c7283977bc2dc56570ee526f04f164053f195, branch feat/rust-rc-next-slice.
Original owner codex-9czdjh confirms no unpublished successor. The divergent local
78758ec4a7fff95a771e7a6b09e3539ca9a3a4eb is an older abandoned composition,
not a successor; it lacks the f079bd72193 wrapper correction. Never substitute it.

The predecessor contains an atomic HMAC recipe, broker-role/setup integration,
selected identity reads, native-fixture collector changes and SDK wrapper custody
controls. It is not a complete request-lifecycle migration. Its immutable
RUST_KUCOIN_DISPATCH_SIGNER.md records independent HMAC/native source CLEAR from
claude-4yjlw0 and credential/role/setup CLEAR from claude-m26ps4. Historical test
and CI receipts remain historical; none certify this composition. The wrapper
correction f079bd72193 and its 67-test/three-file receipt are part of c87.

The old #1973 and #1990 prerequisites actually landed as a392b114d802 and
bd8b5957f3ba; both are ancestors of this tip. That removes the old landing hold,
not the need to adapt to current private/Optional Identity fields, setup facts,
CoreCx and later shared contracts. Use signing_ref() and other current reviewed
accessors; never recreate a raw field, empty ref or legacy resolver.

Before inherited source is published, plain-merge c87 into the replacement branch,
keeping current unowned implementations and the narrow owned broker deltas.
Read-only merge-tree already reports non-doc conflicts in construction.ts,
localSignerSetup.ts, signing.rs/signing_tests.rs, registry/lib.rs and both signing
checker files, plus the regenerated request manifest. This preview changes no
checkout and is NOT reviewed source. Resolve each shared hunk under its owner's
current contract and obtain exact-patch carry review. Preserve the entire current
foreign inventory and regenerate only scanner-proved owned changes. Docs retain
incoming text followed by the exact predecessor suffix once, then this plan.
Do not close #2033 until a replacement PR actually contains the credited inherited
history; cross-link and close the superseded draft then. No cherry-pick, squash,
rebase, source copying, force push, checker allowance or silent assertion rewrite.

## Exact live scope

Current production request inventory is 31 occurrences, signing inventory 41 and
credential-selection inventory 15. They measure different lexical facts and must
not be added together as distinct requests. The current request checker separates
cfg(test) provenance; the predecessor's old 46/37 counts cannot carry across it.
The historical 84-site request plan remains a historical snapshot.

| Path under sdk-rs/venues/kucoin/src | Request | Signing | Credential | Intended owner |
| --- | ---: | ---: | ---: | --- |
| account.rs | 3 | 0 | 0 | Background account/census and revision-bound streamed rows |
| funding.rs | 1 | 0 | 0 | Existing funding history owner and bounded native pages |
| history.rs | 0 | 1 | 0 | Existing history bridge; remove raw presence inference |
| identity.rs | 2 | 0 | 1 | Selected genuine command Read and its terminal |
| lib.rs | 7 | 16 | 10 | Shared issuance, public conditional signing, candles and bullet acquisition |
| secondary_tests.rs | 0 | 8 | 0 | Retained native credential/passphrase/broker oracle controls |
| spot/mod.rs | 11 | 16 | 4 | Account, order census/lookup, fills, bullet and shared request helper |
| spot/trading.rs | 4 | 0 | 0 | Ordered HF writes and account-identity terminal |
| trading.rs | 3 | 0 | 0 | Futures metadata/preflight, ordered writes and journal terminal |
| Total | 31 | 41 | 15 | No prune in plan or original controls |

Account's three request hits and the trading request macro are conservative
Cx-provenance matches, not four additional physical calls. Preserve their complete
native assertions/reducers. A later pure codec extraction removes provenance only
when its real Context use is gone; renaming a variable or hiding a call is not a
migration. The eight signing hits in secondary_tests remain native evidence.
Adding KuCoin to migrated sign-module scanning also exposes legitimate typed
HmacSha256 recipe declarations and test oracles; report actual residuals instead
of claiming signing zero. Freeze exact before/after maps independently.

The open-PR file audit finds only the assigned #2033 touching KuCoin production
or tests. Shared engine/setup/registry files have other owners: this assignment
confers no permission to replace their current implementations.

## Atomic recipe, construction and the minimum signing inputs

Reuse the predecessor recipe rather than build another HMAC algorithm. Trusted
setup/provider holds selected API key, HMAC secret, selected passphrase and an
optional broker secret. Partner/name/verify are non-secret metadata. Venue Options
and retained exchange options contain no raw copy. Hosted mode must support this
minimum atomic work through the same generic provider contract as other venues;
the legacy one-signature callback alone is insufficient. Hosted admission stays
held until its real multi-operation bridge is reviewed and tested.

One validated dispatch receipt supplies the integer millisecond stamp. In one
atomic SignRequest, ordered operations are: Present the selected API key; HMAC
selected secret over stamp + METHOD + RAW path/query + exact body; HMAC the same
secret over the selected passphrase Credential; optionally HMAC the broker-role
ref over stamp + trimmed partner + that same selected API key Credential. Render
standard padded Base64. Do not Present the raw passphrase or broker material.

Attest method, the two allowed origins, separately encoded transport URL, exact
body, ordered unsigned headers and empty signer headers before provider work and
again before applying results. Require the matching receipt, exact result count
and MAC lengths, no injected provider headers and no CR/LF header values. Keep
native u64 decimal timestamp formatting, including the negative-zero control.
No eager timestamp, cached signed bytes, raw REST fallback or venue provider shim.

Native signed headers are ordered KC-API-KEY, KC-API-TIMESTAMP,
KC-API-KEY-VERSION=2, KC-API-SIGN, KC-API-PASSPHRASE, Content-Type,
X-CHANNEL-API-CODE. Only a complete trimmed partner/name/broker-key triplet adds
KC-API-PARTNER, KC-API-PARTNER-SIGN, KC-BROKER-NAME and KC-API-PARTNER-VERIFY.
Verify defaults true; false remains false. Incomplete/blank triplets suppress all
four broker headers and the broker operation. The request and broker MACs share
the stamp; the passphrase MAC has no timestamp in its preimage.

The inherited KucoinBroker role is Optional UTF-8 with Rust str::trim. Blank input
installs no ref; preserve U+FEFF as native trim does. Secondary aliases the primary's
one broker-role ref, never its own supplied broker material or a cross-slot lookup.
Wrong role/algorithm, malformed declared refs, Primary alias, cross-role alias and
alias encoding/trim fields refuse before mutation. HMAC-only broker refs cannot
Present or interpolate Credential. Generic policy/schema/resolver changes need
m26ps4/current core carry review; declare no new venue-local role authority.

## Credential roles and request custody

Native private GETs choose a usable Secondary only while Primary is usable;
otherwise Primary. Declare that once through immutable Slots/Selection policy,
not repeated method-based key checks in the venue. Native writes/DELETE/private
bullet are Primary, and Futures public catalog/depth/candles/public bullet are
conditionally Primary-signed even when a Secondary exists. Spot public remains
unsigned with X-CHANNEL-API-CODE followed by Content-Type. Keyless/public-only
fallbacks preserve the native gates; test actual behavior rather than infer it
from a registered opaque Primary alone. Invalid supplied read credentials are
checked before public-only/testnet startup gates. Absent secondary passphrase
inherits the primary opaque ref; explicit empty must refuse, never silently inherit.

| Funnel | Shared authority and preserved behavior |
| --- | --- |
| Futures/Spot identity and remaining finite command reads | SelectedRead with explicit native Read/Primary role, genuine command admission and retained owner; validate delivery before decode, one Context terminal, no numeric fallback on shared refusal |
| Futures public catalog/ticker/depth/candle requests | Fresh keyless or explicit Primary-selected Background/command Read as native; keep stream paths, keyless headers, generation and timeout cleanup |
| Spot discovery/candles/local settings/history already adopted | Preserve their landed Context owners, headers, row/Store ordering and refusal controls; compose signer changes without regressing or re-migrating them |
| Futures account and Spot balance/order census | Background Scope plus real selected Read legs or the appropriate complete Plan; stable revision/census owner, streamed-row admission before Store, native page/retry and loading-flag cleanup |
| Spot client lookup | Cache fast path and selected paged order census; on native transport failure retain diagnostic then cache-backed found result, under the original owner |
| Funding/history/public-trade reads | Keep existing shared history/Completion owners and accepted-only failure. Preserve window/cursor/boundary/capacity semantics; authenticated public trades retain their native Secondary-or-Primary distinction |
| Bullet acquisition | Real Session acquisition HTTP authority tied to existing generation/supervisor retirement; private SessionAuth selects Primary, public signing follows native family policy. No manufactured login, Bearer bundle or socket migration implied |
| Futures and Spot ordered writes | Genuine WriteAfterReads Preflight, retained queued ownership, serial Plan and journal; one Slot per actual call, no Mutation group for a sequential request list |

Every entry obtains authority before queueing, generated identities, provisional
Store effects or HTTP/signing. Shared refusal owns the only terminal. A child
leg uses its actual released delivery for continuation, never another command's
callback or a numeric ID. Validate final DATA, stream items and stream errors
before codecs, settings, row/Store publication, queue wake or terminal. Preparation,
provider and integrity refusals remain typed; do not cast them into network failures
and spend native transport retries. Late/duplicate/stale callbacks are inert.

Native raw Cx::request emits timeout_ms=None. The JavaScript managed transport
has a 20s fallback, which is not an engine admission deadline or a cross-host proof.
The overseer APPROVED managed 20s per physical call across admission/Prepare/
provider/Bind/I/O, an unbounded serial root and no whole-command cap. Pin native
transport behavior and disclose the new preparation/eligibility bound in CALLERS.
Futures has a native 10s account-metadata wait from initial queueing, 20ms queue
poll/continuation delay, and cancel-proof 100ms polls. Preserve them through retained
Preflight readiness and serial continuation/condition APIs, not a venue-local
second authority. Any new receipt, eligibility or timeout precedence needs an
explicit CALLERS disclosure and owner ruling before source.

## Futures/Spot continuation and error facts

Futures preflights the input before the credential refusal. It then holds a FIFO
job until required account metadata becomes ready or the native 10s wait ends.
Preflight capability/signing availability must be established before effects and
before an amendment can cancel its source. Metadata queries keep their independent
native account owners; do not relabel them as fictitious calls of the command.

Futures mutation and validation GET steps retain their immutable path/body/client
identity, mode checks, source-fill/cancel proof and accepted IDs. A 404 ordinary
cancel may issue one stop-order fallback; that is a distinct native next call,
not permission to resend an Unknown write. Cancel confirmation validates exact
order identity and fresh fills, polls up to its native budget, and retries failed
proof reads twice with 100ms delays. Replacement amount/full-position-stop logic
and fill changes immediately before placement stay unchanged. Independent native
Futures CancelOrders siblings continue after a failing sibling: the overseer rules
complete Plans for that path, as for BingX, with the complete native list reserved.
Dependent replacement/placement and native fail-fast Spot sequences use serial
Plans and stop as native requires. Never seal the first independent sibling's whole
job or introduce continue-on-error into a native fail-fast path.

Spot HF jobs issue sequential Place/Cancel/Amend calls. Preserve orderId/newOrderId
parsing, fallback to the old amendment ID where native allows it, revision/heal
order, caller receipts, no generic retry and no false rollback of earlier writes.
Both native reducers strip only Rejected/NotSent after effects/accepted IDs. Shared
journal projection owns the adopted command terminal; record differences in
CALLERS rather than rewrite facts, reset IDs or rebuild a native outcome journal.

Native Futures parsed code 400005 selects one immediate freshly signed resend per
request, regardless of HTTP status; the retained retried bit prevents a third send.
The fallback cancel is a new call with its own native bit. Code 400002 is AuthInvalid
and does not take that native retry. Do not mistake an illustrative core-classifier
comment about clock skew for KuCoin's executable contract. Spot has no generic
400005 resend. Account uses a shared retry counter (<1 for 400005, <2 otherwise,
reset per page); history/funding preserve their native eligible statuses, shared
counter, 1000*retries delays and history scheduling/350ms spacing. These are distinct
ledgers, not one new family retry policy.

Managed Futures writes should use the shared classify_write fact and a genuine
RefusedResend grant followed by the existing serial resend selection, without a
Session: this HMAC clock/signature refusal requires fresh signing, not GRVT login.
Declare attempts=2 on the retry-eligible Futures Mutation call and attempts=1 on
Spot writes. Serial Reads use attempts=1; native Read retries become new Read calls
under the retained native ledger, with validated DATA or released-failure provenance.
Do not enable session-bearer merely to admit Read2, manufacture a Session, or treat
retry selection as permission for a second auth or transport write replay.
Transport, preparation, integrity or an unparsed response grants no write retry.
The overseer APPROVED the narrow status_exempt_refusal/RefusedResend exception
for parsed 400005 with exact native eligibility, including 408/5xx. Code 400002 is
AuthInvalid/no retry, Spot gains none, and other/unparsed 408/5xx remain Unknown
with no write retry. Codec/eligibility originals and current API review still gate
source; this is a KuCoin-specific ruling, not a blanket widening.
Use a venue CodeTable, not a new engine status ladder. Preserve native error text,
400002/400005 distinctions, HTTP418 classification, explicit refusals and unknown
body/ACK cases or obtain and disclose an intentional shared-contract difference.

## Original controls and source gates

Before new source, extend/re-execute the preserved c87 adoption tests against the
actual composition. Record genuine first assertion REDs separately from schema
rejections, missing API, compilation and unexecuted suffixes. Keep the old native
HMAC/header/financial assertions and wrapper material-custody tests intact.

1. Real installed local/recording providers, selected Primary/Secondary and no-key
   controls; actual Prepare/Sign/Bind/Claim for managed adoption, with no fabricated
   provider result, receipt, Context, grant or signer capsule.
2. Both origins, raw/encoded query divergence, exact body and 7+4 ordered headers;
   trim/BOM/partial broker triplets, verify false/default, passphrase fallback and
   explicit empty, aliases, wrong algorithms, provider-tampered receipt/result/header.
3. Same dispatch stamp for request/broker, changed stamp/fresh provider work on each
   eligible 400005 resend and unchanged recomputed passphrase MAC. Hold signing and
   route preparation to prove time/freshness at actual release, not eager queue time.
4. Futures 400005 at200/401/408/503, plus non-400005/unparsed bodies, second 400005,
   code 400002 and provider/transport/integrity failure. Spot negative controls must
   show no newly invented retry. Account/history shared-counter and page-reset cases.
5. Real256 Command owners, exact first retirement, recovery, then a fresh 256th peer
   and validation of all 256. Clock/NaN, receipt, duplicate owner, queue cleanup and
   native local/empty/unknown-market precedence; no cloid or Store effect on refusal.
6. Queued metadata readiness/10s boundary, foreign callback wake, serial condition
   waits and native 20/100ms delays; independent cancel siblings versus dependent
   replacement stop,404 fallback, fills at each cancellation/replacement boundary,
   partial accepted IDs and operation-versus-command journal outcomes.
7. Stream item/final/stream-error revalidation, malformed accepted/rejected rows,
   account revision/census retirement, accepted history rows on later failure and
   Spot lookup diagnostic-plus-cache result; no completeness invented by a codec.
8. Bullet provider failure, prepare expiry, old acquisition after retire, socket
   generation/welcome/pong deadlines and per-feed startup order; no revived token
   or stranded token_pending/loading flag. No socket-lifecycle zero claim by proxy.
9. Hosted trusted-setup atomic batch and all denial paths, secret erasure/no retained
   options copy. The old delegated callback remains explicitly unsupported until
   the generic provider bridge proves every required operation.

| OPEN gate | Owner proposal and disposition needed before source |
| --- | --- |
| Inherited current API/source composition | This owner carries c87; original HMAC reviewer 4y and setup/role owner m26 (or current assigned independent replacements) review exact shared resolutions. No old CI/source CLEAR automatically covers them |
| 400005 exception, call bound and write/read attempt geometry | RULINGS APPROVED for native eligible parsed 400005 at any status and managed 20s calls/unbounded root. Current serial/complete Plan API and codec/original review remain; existing hook/default classifier stay generic |
| Independent cancel siblings after uncertain failure | RESOLVED-PENDING source fit: overseer assigns native continue-on-error cancellations to complete Plans, as for BingX. Reserve the real complete call list; native fail-fast dependent chains remain serial. No serial continuation after an uncertain write is inferred |
| Account/census/history delivery authority | Reuse current account/read/history APIs and owners; request a shared seam if caller scope or retained delivery is missing rather than invent one |
| Bullet Session HTTP authority and stale owner cleanup | Coordinate credential/session core m26 and native socket reviewer55ao7t; maintain supervisor behavior and separate REST/socket ownership |
| Hosted atomic bridge | Shared provider/host owner m26; minimum operation-needed material per Sam's signing model. No per-venue legacy callback escape hatch |
| Size/features | Isolated KuCoin Futures, KuCoinSpot and combined on actual tip; any shared engine change also pairs Bybit/Bitget before READY. No assumed headroom |

Source remains held until the relevant plan/original/current API reviews and
rulings close. Original controls may land as a separate opt-in checkpoint when
intentionally RED, following the existing private-adoption-originals pattern.
Only enable serial-plans/condition-waits or another non-default feature when a real
adopted caller uses it; list every enabled feature and resulting measured budget.
No session-bearer, serial-pacing, dispatch-nonce or Mutation group is assumed.

## Verification and landing

Plan phase changes this document and its index only. No source/test/Cargo/map
changes, builds, size measurement or runtime claims. Exact scoped maps and a
read-only inheritance/conflict receipt identify the next work; no production
resolution is performed to manufacture a plan receipt.

For inherited source and each later adoption checkpoint, run touched Futures/Spot
and registry/wrapper controls, feature-off/on controls, strict clippy/fmt, full bans
and request/signing/credential/socket ratchets on the actual composed tip. Shared
public API changes require workspace all-targets checking. Retain old test bodies;
prepared collector changes need exact assertion-preservation review. Preserve
foreign inventory bytes/order and get exact-patch CLEAR for manual non-doc hunks.

Freeze/push once for one PR-triggered Rust run. CI is the family-size record;
combined needs one local composed measurement under the current carry rule.
Measure paired Futures/Spot and combined as required for the final adopted inputs;
shared engine work also measures Bybit and Bitget. Keep shipping profiles/budgets,
record raw/gzip/hash/input seals, and delete each own scratch target after a build.
No job rerun, duplicate dispatch or pattern kill. READY goes to 8x5tm1 only after
review and required gates; owner merges only after exact-tree GO and fresh guards.

## Inherited original checkpoint revalidation

The c635adf7120 original-only checkpoint is now a plain-merged ancestor, preserving
its authorship; no c87 production source is imported. Its README conflict keeps
the complete current index plus the exact historical signer-plan row once. The
historical signer plan is retained as a snapshot; this document owns current counts.

Current fixture carry uses signing_ref(), explicit credential/signing refs for
hmac_sha256_work and nonce=None in the HMAC-only SignReceipt. Two absence assertions
use the equivalent contains_key form required by current clippy; the broker header
test adopts c87's already reviewed name, avoiding an accidental preserve_order ban
token without changing its oracle. Reversing only these explicit fixture changes
reconstructs both entire original test blobs. No native expected bytes, metadata,
provider-count assertion, alias/MAC expectation or failure assertion is weakened.

| Executed target | Current evidence |
| --- | --- |
| KuCoin + Spot original target | 11 tests: four actual provider-custody assertion REDs, seven GREEN native/provider controls, zero ignored |
| Futures without Spot | Nine tests: three provider-custody assertion REDs, six GREEN controls, zero ignored |
| Broker role specification | Eight tests: six NEW-CONTRACT runtime schema refusals, two GREEN existing provider/role controls, zero ignored |
| Strict checks | Both scoped original targets pass clippy -D warnings; formatting and patch whitespace pass |

The native REDs fail because the eager native sender performs no recording-provider
batch. A genuine LocalSigner multi-operation positive control passes; no failing
provider mock, setup panic or compilation error is counted as a native RED. Broker
role/policy decoding fails before downstream alias/MAC assertions; those suffixes
remain unexecuted and unclaimed. The missing role is not a completed constructor.

The first compile attempt exposed the changed HMAC helper/receipt APIs and is
excluded from runtime counts. The later broker clippy failure named the two old
get/is_none predicates; the equivalent assertion update was followed by a fresh
broker runtime and strict check. Native fixture bytes were unchanged by that lint
correction, so their successful runtime/strict receipt carries exactly. Own scratch
targets were removed after every review run. No Cargo, production, inventory or
provider activation changed, and this branch is not pushed while its original
assertions remain ungated. Relevant source/current shared-hunk reviews still gate
production composition and the later full request-lifecycle controls.


## Local managed-trading originals

Before request-funnel source, six compiling controls extend the existing trading
fixture through a child module; every prior native assertion remains unchanged.
On the inherited signer composition they execute as four intended assertion REDs
and two native GREENs, zero ignored. The REDs pin real Primary preparation, the
new 20 s bound, unclaimed DATA, and non-finite entry before queueing. The bound
suffix is gated behind the missing preparation; it is not claimed executed.
Native positives pin parsed 400005 at 200/401/408/500/503/599, one resend only
with a fresh timestamp/signature and frozen method/URL/body, plus continue-on-error
independent cancels after an uncertain sibling. Later capacity, streamed-data and
Session originals remain separate gates. These local controls are unpushed.

The child module alone is gated by the non-default private-adoption-originals
feature; all 40 inherited trading controls remain enabled in default CI. Run
cargo test -p safe-cex-venue-kucoin --features private-adoption-originals
--test trading trading_context to execute the six controls. The resend positive
also checks no third send after the native terminal timer for every status.
The prior four RED/two GREEN receipt predates this additive gated suffix; no
new execution is claimed by the configuration correction alone.

The inert publication is isolated on landing tip 817a488c9d2. Its only venue
changes are the empty non-default feature, the child include and this new child
target. It copies no predecessor production or fixture plumbing. The actual
incoming 38 trading controls pass unchanged by default; the separate c87-based
local checkpoint also contains two predecessor helper unit controls. The isolated
opt-in execution reproduces four genuine assertion REDs and two native GREENs,
including the new post-timer no-third-send assertion. Scoped fmt and strict
clippy pass. Formatting the new child changes no assertion or native test body.
