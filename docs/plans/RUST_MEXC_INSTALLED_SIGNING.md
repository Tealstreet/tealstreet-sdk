# MEXC installed signing and private request custody

Status: local implementation in progress, unpublished; full PR review follows
plan, originals and source together under the overseer speed instruction.
Owner codex-mxb4mq, reassigned by the overseer from kuhwxi's later queue;
reviewer codex-hcmq3f. Base eeb9d170f640 on origin/feat/rust-rewrite.
One venue PR combines plan, originals, source and correctness qualification.
WOO remains clean on feat/rust-woo-trading-signing at d402e9e4637, waiting for
the two assigned pacing prerequisites. Resume it after their actual landings.

## Ownership and boundary

No open MEXC implementation PR was found. The open cross-family HMAC plan
#1718 is documentation. The old rust-mexc worktree is on a standby branch;
kuhwxi's Aimux session is not running. Historical dispatch-signer branch
62633039708 contains docs and eighteen original controls, no production source.
Treat those controls as references; adapt only genuine managed-public setup.
Do not import historical engine changes or claim old reviews cover this source.

Current inventories contain22 signing,12 credential and13 production request
occurrences. Signing includes seven secondary test fingerprints, so22 is not
twenty-two independent production senders. Freeze foreign map entries and order;
remove only exact retired MEXC occurrences after the actual scanner proves it.
Existing public Markets/Tickers/Depth and caller Book/Candles custody stays intact.

Cut over all credential-bearing users atomically: the shared account/trading
helper, direct private-history signer, account identity and private WS login.
No raw fallback, recipe-only bridge activation or keyless signed Context.
Scope includes MEXC source/tests, its registry arm and trusted setup mapping,
one construction row, direct bridge/setup controls, docs and removal-only maps.
No engine, crypto-provider, checker, host transport or socket-mode migration.
Expected size is roughly3000 runtime lines plus controls/fixture adaptation;
CI is the MEXC family record. No local combined while headroom exceeds200K;
use the standing venue allowance only if the CI record requires it.

## Installed authority and persisted identity

Consume raw credentials only at trusted construction and pass installed refs
to the venue. Primary credential trimming follows Rust White_Space semantics;
the Primary secret and all Secondary bytes stay exact. Read selection is per
native caller, never inferred solely from GET: account reads and amend-read
polls may use Secondary, private history stays Primary, writes and WS login
use Primary. Public trade history is unsigned. Invalid configured Secondary
must not fall back to Primary. Public-only cannot start private work.

Native identity has a distinct eligibility rule: a saved nonempty raw Primary
key plus a complete Secondary may validate even without a Primary secret.
Preserve that capability using a credential-only Primary and a Secondary Read;
do not manufacture Primary signing authority. Other private history, leverage,
startup and trading gates retain their own native usable-Primary requirement.

Persisted userId is mexc: plus the first12 SHA256 bytes of the RAW Primary key.
REST signing trims that key, so hashing the provider-presented trimmed value
would silently remap accounts. Proposed trusted MEXC compatibility setup computes
the existing mapping before trimming and carries it as Primary stableId through
the existing metadata channel. Venue execution reads only that stableId; no raw
key hashing or provider key-hash escape. Both JS and native setup need the same
literal vectors, including surrounding whitespace. An injected provider supplies
its stableId explicitly. Missing metadata refuses identity visibly without
altering the key or selecting the Secondary's identity. Exact setup/API review
must close this mapping before production; do not remove persisted capability.

## Exact REST and login recipes

REST preimage is selected credential bytes + integer protocol stamp + exact
payload. Use provider-private PayloadPart::Credential followed by byte parts,
with installed credential and signing refs. GET payload is the native sorted
encoded query; POST payload is the original serialized body, never rebuilt.
Attest exact method, host, path/query, body and static headers before provider
work and again before applying the result. Require exact result shape/receipt,
nonempty safe presented key,32-byte HMAC and no unsolicited provider headers.
Keep header order ApiKey, Request-Time, Signature, Recv-Window, Content-Type.
Window defaults20000, clamps1..20000; signature freshness uses the selected
window and never extends the original physical deadline during preparation.

Private open sends the plain ping before provider-backed login. Login JSON
remains method=login and param apiKey, reqTime STRING, signature in that order.
Its preimage is Primary credential + the same integer stamp + empty payload.
Use Selected SessionAuth and Cx::send_signed with exact captured SocketId;
retain the existing legacy socket generation/retirement and Required native
login-ACK timeout. No managed feed registration or coexistence seam is added.
Old-socket provider completion must not send on a replacement socket.

## Physical ownership and reducers

Admit original caller Read roots for identity/leverage/history before I/O;
retain one selected binding and exact Context in Pending. Every private account
observation has its own Background20s/Never authority, including each page.
DATA validation precedes streamed rows, status, diagnostics and reducers.
Publish accepted state while live, retire exact custody before successors.
Preserve full/partial account joins, revisions/tokens, stale fencing, settings,
financial scaling, history windows/limits and accepted-page recovery. Expired
owned failure cleanup uses exact retirement rather than a live-DATA check.
Duplicate/foreign/revoked handles cannot advance or retire independent work.

Trading retains genuine original-entry Preflight/Plan custody and one FIFO;
never reconstitute Command entry or issue under an unrelated timer callback.
Amend cancellation, up to six native reconciliation reads, definitive remaining
quantity and replacement stay on the same owner with captured Read vs Primary
write binding. Never resend a mutation on ambiguous failure. Pure ACK decode,
truthful write fact, effects validation, Store/diagnostics, then exact retirement.
Mandatory bounded reconciliation captures the native IDs and financial intent.
No accepted prefix may become NotSent; no replacement after unproven cancellation.

Native lists are not capped at256 calls: batching chunks50, mixed order kinds,
settings rails and definitive batch fallback require an API-fit audit against
complete/streamed Plan bounds. No fake split Commands or dropped native sends.
Only SUBMIT/BATCH joins physical pacing:450ms for singleton and1500ms for a
multi-order release; the latter debt survives a following singleton. Cancels,
leverage, conditional placement and amend GETs do not acquire that clock.
Code510 retains its explicit response-derived cooldown. Local NotSent charges
no physical slot, as approved in the historical disposition; test immediate
following valid release and held-provider/final-Claim spacing separately.

Current serial Plans reject nonzero pacing and inherit one root selector.
Do not implement venue-clock release arbitration or assume WOO's pending APIs
already exist. Reviewer must settle the exact MEXC plan/call mapping before
trading source; if the same shared prerequisite is needed, consume its actual
landing instead of editing core. Independent recipe/read design can proceed.

Each physical call is20s/Never with original first-admission wait where the
shared API requires it; no guessed aggregate serial clock or caller-wait renewal.
Typed local/admission/precommit/released failure matrix stays explicit and
separate from physical ownership. Nonfinite time creates no new work/timers;
only exact issued cleanup is allowed, without damaging a peer/cadence timer.
Once-only retirement notifications must release queued successors under genuine
preflight readiness, never native pump with borrowed callback authority.

## Controls and delivery

Unchanged-runtime baseline runs the scoped MEXC crate before new controls.
Adapt historical controls only where their public setup expects retired raw
HTTP; preserve every existing financial/wire assertion and payload. Pin literal
REST/WS HMAC, window/trim behavior, Primary-only history, Secondary amend reads,
credential-only Primary identity, ping/login order, no-private public mode and
the persisted raw-key fingerprint. Count real assertion RED separately from
setup/compilation errors and do not label legacy handoff as final-Claim proof.

Source-stage controls need genuine installed providers versus different valid
raw contrasts; actual Prepare/Bind/Claim and parked signer; held route/provider
expiry/equality; exact cleanup ledgers, healthy peers and full Store/actions;
once-only original terminals; mixed financial batch/fallback/partial effects;
live publication before retirement; duplicate/nonfinite no work and all-credit
reuse. Preserve old fixture assertions, financial payloads and clocks; review
any necessary managed collector adaptation as an exact artifact before adoption.

One hcmq3f CLEAR plus green correctness build/tests/strict Clippy/fmt/bans and
MEXC replay permits a guarded owner merge under speed mode. CI supplies family
size; no auditor/CPU/certification wait for an eligible venue-only slice. Shared
source/checker/additive-inventory work is outside this scope and its own gate.

## Current unchanged-runtime checkpoint

On exact eeb9d170f640, scoped native qualification is223PASS/0FAIL/0ignored
across18 targets. Historical eighteen controls have been refreshed locally:
only five public startup collections now release genuine managed preparation
at the original0/100ms clocks. Reversing those statements reconstructs the
entire historical target; existing assertions and raw financial literals stay
unchanged. Current outcome11nativeGREEN/7intended assertionRED/0ignored/filter.
Five REDs expose eager signed private transport; one exposes queue-time450ms
spacing after delayed handoff, and one exposes a NotSent refresh consuming the
next native slot. No compile/setup failure is counted. These legacy witnesses
do not yet prove installed-provider ownership or final managed Claims.

Hcmq3f cleared the plan and first originals at7fba53a89f0. The overseer then
authorized local source without waiting for another preview: review is on the
full venue PR. Trusted-ID direction is CLEAR; no source or runtime clearance
is inferred from that plan verdict.

## Local implementation and remaining shared prerequisites

The independent local source now includes immutable provider-backed REST/login
recipes; installed-only venue options and MEXC trusted JS/native stableId setup;
selected account/identity Read ownership; and Primary-only private-history roots
with exact live page admission/retirement. Account publication is live, with
physical successors deferred until the completing observation retires. Public
trade history stays unsigned under its caller root. Existing identity assertions
and literal IDs/HMACs pass with genuine installed providers and managed release.

Current scoped evidence:16 Rust identity/Read/history controls PASS, three native
trusted-setup controls PASS, eight JS setup controls PASS, MEXC-only construction
build PASS and strict library Clippy PASS. Baseline223 stays historical: the full
current family, existing fixture migration, inventories/bans and replay have NOT
yet qualified. The full-target original18 remains the unchanged-runtime RED
checkpoint; do not relabel it as green after the source changes.

Trading remains on its legacy FIFO/pacing, using installed provider recipes only
as local wiring. Its temporary legacy sign_http attachment is NOT a final
managed trade funnel and cannot be published. Fe7 confirmed THREE actual core
landings are required before that scheduler source can qualify: dazzy3's
release-domain admissible sets, fe7's SerialCall.pacing, and dazzy3's separate
Cx::defer_pacing slice. No own engine work or venue-clock substitute.

One submit domain declares450ms. SUBMIT/BATCH selects outer gap0 for an original
single-order job and1500 for EVERY call from an original multi-order job,
including a singleton final chunk and fallback. All other calls are unpaced.
Code510 maps to defer_pacing(context,1500) in its exact released response
custody: extend its resolved groups to max(floor,response-now+1500), no Claim
or terminal. The next head waits on that shared floor. Actual landings plus
consumer contract controls remain gates; a proposed API is not a landed one.

## Independent Read checkpoint

Private precommit recovery is typed: Capacity/TimedOut, or Invalid with
Network/Timeout/Internal, retain native delayed account recovery. Other owned
finite refusals retire once and warn without starting new recovery. Released
failures retain native handling. Selection/configuration failure never borrows
the transient row; nonfinite delivery only cleans up exact ownership. The new
control drives an actual prepared Mode read for eleven typed combinations using
one identical message, rejects duplicate rearming, and completes a real sibling.

Local-read fixture adoption captures the actual preparation and permits exactly
its one Cancel on successful completion. All remaining no-new-HTTP assertions
stay. The paused-capacity control holds254 depth peers, completes its genuine
429, then fills the two released credits with independent candle reads: depth
admission is correctly paused by that429. It retains all256-peer survival and
slot-reuse assertions. History collectors release genuine managed pages; their
response/failure observation follows the original query observation rather than
the shared startup helper's earlier100ms clock. Startup remains at100ms and all
financial payload literals/result assertions are unchanged.

Current scoped strict Clippy covers the MEXC library and installed_reads,
identity, history and request_context_local_reads. Whole-family qualification,
remaining private fixtures/unit probes, inventories and replay remain open;
this local checkpoint is not publishable while trading uses temporary legacy
attachment. No full-target18-originals GREEN claim is made.
