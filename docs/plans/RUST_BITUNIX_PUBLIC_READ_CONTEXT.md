# Bitunix public REST Read authority

Owner: codex-9czdjh. Frozen inventory/source: `749a5677d0e9067e3ee58ae76893446f37dfd39c`.
Status: managed public REST adoption; native verification and independent review, before CI/merge.

## Selection and collision audit

The frozen Bitunix inventory is 23 occurrences / 20 fingerprint rows / four
files. Public markets, tickers and command candles form the largest available
single-request slice: **eight occurrences / seven rows**. The expected isolated
result is 15 occurrences / 13 rows / three files. Separately landed #1903 removes
five history occurrences / four rows, so integrating that landing before this
slice yields 18 -> 10 occurrences, 16 -> nine rows and three -> two files.
Do not count #1903's removal as this slice's work.

The fresh open-PR audit checked 55 file lists: neither Bitunix lib.rs nor
account.rs nor the proposed new test is claimed. #1771 has landed; its socket
implementation remains outside this slice. Source ownership notices went to
codex-5c57jl and claude-19pt8s before any production edit. Recheck open PR files
and unpublished claims before test/source changes. Private signer adoption is a separate slice using the recipe already handed
off by claude-19pt8s; this public conversion changes no signer or identity setup.

Both owners subsequently confirmed the public/Pending/defensive-candle regions
clear. Bitunix is excluded from codex-5c57jl's managed socket rollout. Preserve
its existing supervised socket path; no managed-socket adoption is implied.

Larger raw maps are not larger independent conversions: KuCoin lib.rs and Spot
files overlap frozen #1922; BloFin history/lib overlap #1865 and its private
account/closed/trading work is reserved by the paired HMAC adoption; Bitrue's
shared dispatcher and private trading need the signer/Plan conversion, and its
local history/account refusals are already covered by #1906/#1909. Bitunix
trading's eight occurrences include four conservative macro occurrences and
private writes; they cannot be removed by this public HTTP slice. None of these
sites is renamed or exempted to improve the count.

## Exact selected inventory

Paths are relative to sdk-rs; line numbers locate the frozen source.

| Path:line | Fingerprint | Count | Shared destination |
| --- | --- | ---: | --- |
| venues/bitunix/src/lib.rs:139 | `8e9c6f91ed3af3c6827d9d5f84fc2f7e39dd4919fac482cbe69b6cd5384d316e` | 1 | Markets background Read, managed unsigned GET, streamed data envelope. |
| venues/bitunix/src/lib.rs:151 | `2550ad03895c0f6a051fbcdc81d13f2b6bf8e622cec5166292c130c02207a2d8` | 1 | Tickers independent background Read, managed unsigned GET, streamed data envelope. |
| venues/bitunix/src/lib.rs:715 | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Candle result through the original command Read root. |
| venues/bitunix/src/lib.rs:716,823 | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Decode/transport failures through typed original-root completion. |
| venues/bitunix/src/lib.rs:980 | `934e56fa93ceed83a6210d98437ca0fc4586e45e66981b562f8660a11afa6bc4` | 1 | Unsupported interval after genuine initial Read admission, before I/O. |
| venues/bitunix/src/lib.rs:997 | `d3cd5e4283cb70e8237d488a2da4477a9f90315cbeb8a47e5ce612849d98cff3` | 1 | Candle managed unsigned GET with the native URL/body/header bytes. |
| venues/bitunix/src/account.rs:562 | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Defensive Candles failure arm uses the original typed Read root; it is not a separate reachable signed-candle flow or RED witness. |

The six lib.rs rows disappear only after their actual dispatch/terminal calls
move. The account row is a real defensive terminal conversion, not deletion of
an assertion or a binding rename. No scanner change is required.

## Full remaining inventory disposition

The existing exhaustive snapshot table in RUST_BITUNIX_REQUEST_CONTEXT.md has
the same 20 keys at this pin. After the selected seven rows above, all remaining
rows stay frozen as follows:

| Path | Remaining fingerprint rows | Occurrences | Disposition |
| --- | --- | ---: | --- |
| account.rs | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc`, `37e060689327c1ad79d592ea1a9079e30ef97cf58ffc0b65967b779b8d5b97fa` | 2 | Signed buffered/streamed dispatcher; paired signer/managed private adoption. |
| history.rs | `2b6c87682e5a05f8f7b1a34a56d5d415f322353ec9c26d679431d57de7aad64b` (2), `6ae09f4ddf0da52a0d4557bbbbdf7683b02309fd5b83052f95902ca84e4e8a69`, `e6976f28ef1c6418ff7a12ae23db6119dc6a593aaa5ef6e736e97acde6ba1477`, `ea50153d8d70fa0cb55007393aadf62c3666dbb5e2f9d6c5d64bedb72d3299e5` | 5 | Independently converted by #1903; preserve its actual landing/prune. |
| trading.rs | `5c8327b0c32fec41a26bebe2be6242f01f42dd559db6c6ed0d3a44593daa63b9`, `5d6691e305370ef1b5e22426e494d28b28936de80d5d5af079e9deae52eb33f2`, `67b7e6451565bda29255c0f41f9f46bfc8309ec5998b99f6e286840574190e1c`, `f8b99f320746bee1eaad4962337f49532934f311df03900770ee8305e576db60` | 4 | Private mutation admission, shared journal and managed signed calls; separate adoption. |
| trading.rs | `03c9a7632009d78b3353fb0a558c46b45901784ec35f68bf18794743ec39c8fc`, `c75a7e2cea8f3da1f10de9b8aa940fb4fcd3c57ddbccaf72faa83ab9c6e6fcdb`, `e844b5bdfef1fbcdb97ebe211443d5dd47ca3b0ddd4f3c4d63512b2db77c6f81` (2) | 4 | Conservative macro assertions/branches; retain until the real associated private conversion. |

## Authority and files

Use landed `admit_http_command`, `http_context(Scope::Background, ...)`,
`request_managed_with_headers`, `stream_response`, exact Read delivery validation,
`finish_http_context`, `done_context` and `fail_context`. No Plan, execution group,
Session, signer recipe or new engine API is needed for these single calls.

Declare Read / credential None / RetryNever / no pacing / 20,000ms. This is the
explicit managed lifecycle bound, including preparation/queue time, rather than
a claim that legacy HttpRequest.timeout_ms contained a venue timeout: it was None.
Markets and tickers have independent background roots. After a successful market
publication, the native ticker seed gets a new root; it does not borrow an ended
root or become a synthetic two-call command. Existing PublicRetries owns actual
discovery retry timers, independently of each attempt's engine lifecycle.

lib.rs gains Context custody on Markets/Tickers/Candles, typed HTTP failure hooks
and direct managed unsigned issuance. Candle admission occurs inside genuine
FetchCandles before interval validation or work; Rejected/Revoked return with no
numeric fallback. Command candles use validate_read_http_delivery(root, leg)
before response-derived decoding or completion. That validator requires a
Command owner and must never be applied to Background seeds. Markets/Tickers
retain their exact Background Context and call validate_http_context before
staging, Store publication and follow-up sends; the engine's managed callback
entry supplies the exact physical delivery/Attempt gate. Neither validator is
replaced by a numeric request ID or a local authority table. Wrong, stale or
expired DATA grants no parser effects, Store update, readiness, socket opening
or follow-up seed. Background lifecycle retirement happens on every terminal
path before a new attempt. Failure hooks never use a DATA validator.

Typed on_http_failed/on_sign_error preserve engine Timeout/Internal and supplied
preparation VenueError metadata, passing the original error into shared
completion/discovery reporting. Do not stringify and run those errors through
Released::Transport: it would replace their kind with Network. Only a genuine
opaque transport failure uses classify::read(Released::Transport); only a failed
or unproven response uses Released::Response. No retry or auth fact is inferred
from a typed error's formatted message.

account.rs changes only the defensive Candles failure arm to typed completion.
Its signed_request dispatcher, credential selection, auth evidence, account
recovery, reconciliation and valid private reads remain unchanged. No WS codec,
interest, Supervisor, login, heartbeat, socket generation, venue Options or raw
credential plumbing changes are included.

Planned new tests: venues/bitunix/tests/managed_public_reads.rs. Existing native
fixtures may use the shared prepared_actions driver where public actions now
need real Prepare/Bind/Claim; preserve their row/wire assertions and private raw
HTTP controls. Any existing test file needs a fresh collision audit before edits.
The manifest, CALLERS/AUDITS/QUALITY appendices and this plan/index complete the
implementation PR. This planning commit changes only this document and its index.

## Native bytes, publication and errors

Unsigned GETs keep `https://fapi.bitunix.com`, a None body and no credentials.
Production headers stay empty. Testnet retains exactly the one lower-case
`x-simulated-trading: 1` header through the shared static-header API; the declared
static slice also supplies the unchanged private owned-header renderer if needed.
Public read egress remains host Public/read routing; no fake identity or local
forced route is added. Private credential/route selection is not inferred here.

Markets retain OPEN/USDT/API-supported filtering, row-skipped diagnostics and
nonempty readiness. Tickers retain native field mapping, unknown-symbol skips,
and readiness only after a nonempty valid seed. A usable peer row still permits
publication when invalid=true because a different row was malformed; preserve
row-skipped diagnostics. This differs from full-envelope failure, which never
publishes a streamed prefix. Preserve existing socket/private-start ordering:
market commit, Futures connect, ticker request, private_start; public ticker/price
shards open only after ticker seed success. REST validation must not mutate the
socket implementation or add steady-state WS work.

Candles keep the supported interval set, interval-first refusal, no new market or
range guard, raw symbol interpolation, native numeric formatting and response
order, seconds timestamps and quote-volume output. Explicit limit clamps 1..200,
derives start from end and adds `&limit=`; omitted limit uses default 200 to derive
start, honors from_ms when supplied and omits the limit parameter. Native candle
HTTP/envelope/malformed failures remain failures according to the existing
candle codec; no local partial result or retry. Preserve the distinction between
success codecs: discovery error::Code accepts integer/string-integer codes,
whereas candle wire::Envelope uses permissive Num. With valid candle data,
code:null, code:0.0 and code:"bad" currently coerce to zero and succeed. Pin
those native success cases; this slice does not tighten candle code parsing.
Malformed or missing data rejected by the native codec remains unproven. Failed
answers alone pass parsed numeric error evidence to the classifier.

Classifier owner claude-dazzy3 cleared the shape subject to these source gates:

- Extract existing numeric code mappings into **one const BITUNIX CodeTable** in
  a venue codes module, available for later private adoption. Run assert_table.
  No second code ladder, per-public table or message-based evidence is allowed.
- Codec parsing alone proves success; classify::read is used only for a failed
  or unproven answer. Native evidence requires a parsed numeric code; bare HTTP
  errors and message matches pass None. Preserve the native diagnostic message.
  Existing response_data/wire::data may extract bytes/code/message but must not
  decide kind, retry or Outcome on the new managed public path.
- ReadFailure.retry drives discovery retries only; candles always terminate.
  RateLimited uses After plus the existing native pause floor, Unknown uses
  ReadFailure/After, Auth has no retry. Preserve the shared startup pause helper:
  HTTP429 with Retry-After:15 waits 15s, not 60s; business10005/10006 over HTTP200
  retain the 60s floor. The classifier's generic10s default is not that floor.
  Use max(native floor, After), retaining existing shared backoff/jitter for
  retryable discovery reads. Unsigned rows have no RefusedResend or Satisfied.
- ReadFailure.retry=None must NOT call PublicRetries::failed_after/public_failed,
  because that helper always schedules. Emit the supplied typed failure via
  existing diagnostic APIs, keep the seed unready and issue no retry timer or
  replacement root. This is an explicit change from legacy retry-all behavior
  for auth/refused errors. Recovery requires caller-driven fresh venue initialization under the existing
  lifecycle; no new retry command, local resumer, stopped-state registry or status
  ladder is added.
- **Actual #1923 is required before public AuthInvalid/10007 classification**:
  otherwise the current engine increments auth_failures on a credential-free
  path. Wait for actual landing or explicit reviewed-stack permission; do not
  suppress auth locally or introduce a second public table to evade the seam.
- Existing private legacy classifiers are separate debt, not a new fallback for
  public calls. A table extraction that changes their behavior needs explicit
  paired review; do not silently expand this slice. Public work does not close
  private P0 row33 or claim managed signer readiness.

Caller changes to document with implementation: managed transport handshakes,
typed preparation/admission errors, the declared lifecycle deadline and the
shared read error/retry precedence, including no auth-counter effect on unsigned
reads after #1923. Native successful rows/query bytes remain unchanged. Preparation
errors and engine Timeout/Internal preserve supplied typed fields through
shared completion/reporting and cannot infer auth from text. Retry suppression
for nonretryable discovery failures must be documented with its diagnostic and
unready state; successful candle codec coercions remain native.

## Plan, original REDs, implementation and proof

1. Commit this inventory/plan and index on the frozen source; independent source,
   caller and full-map plan reviews must be CLEAN. Resolve unpublished file claims
   and the classifier/#1923 source gates before implementation.
2. Commit compiling tests first on frozen749, with the genuine Venue mutation
   guard delegated. Assert managed Prepare rather than raw HTTP for each public
   request; test no publication or next request before exact delivery authority.
   Pin original assertion REDs separately from existing compatibility GREENs.
   No missing proposed API/enum compile error counts as RED evidence. Tests may
   select raw or managed actions through engine test support while keeping the
   authority assertions frozen; they must not manufacture Contexts.
3. Required authority witnesses: real Prepare/Bind/Claim and pre-release refusal;
   bad route/expired claim; exact response Attempt and foreign/stale delivery;
   candle initial invalid clock/capacity/duplicate/reused-ID refusal; no Store
   mutation or I/O on denial; distinct typed engine deadline, delivered undecodable
   body and preparation failures preserve metadata. More than256 fixed-clock
   Background seed success/failure/preparation/queue-refusal cycles must retire
   each Markets/Tickers root and recover, in addition to candle owner reuse. Capture typed engine
   capacity errors from admission, not the pre-#1877 display literal. The account
   defensive terminal is reviewed, not represented as an executed signed-candle
   command path.
4. Compatibility controls cover production/testnet headers and unsigned route,
   byte-exact explicit/omitted/zero/oversized-limit candle URLs, native rows/order,
   unsupported interval precedence, usable+malformed peer rows versus failed
   envelopes/no-prefix, empty seeds and separate candle-code coercion success.
   Pin discovery HTTP429+RetryAfter15s versus business10005/10006 60s floor;
   public10007/401 after1923 emits no timer/new root and affects no private auth
   budget; candles never retry either failure. Keep private initialization/shard
   ordering, signed REST and existing supervised socket behavior. Native codec
   body failure on code0/2xx remains unproven. No tightening of the existing
   candle code coercions is hidden in the error-classifier migration.
5. Move only the declared calls, prune the exact eight occurrences/seven rows,
   preserve incoming manifest entries and separately landed history prune. Run
   both request-context checks, all touched Bitunix and Binance native targets,
   scoped strict Clippy/fmt/bans, and independent source/caller/full-map reviews.
   The implementation PR needs normal CI including the standalone Bitunix WASM
   dependency graph, size and existing replay/correctness/memory gates. Use CARGO_BUILD_JOBS=2;
   no local WASM/timing build or blanket root-suite run.
6. Integrate the actual prerequisites and current landing before final proof.
   Report missing seams or genuine failures; do not invent authority or waive
   gates. Request a train slot when green; fresh separate own-path/full-incoming/
   exact-key guard and separate head-matched merge are mandatory. No merge window
   is implied by this plan.

Architecture gate: **No per-venue fix for a cross-cutting concern**. The venue
supplies native codecs, declared constant headers and its one native code table;
engine admission, transport, error precedence, retry evidence, publication and
completion remain shared. Missing shared behavior is an owner request, not a
Bitunix bypass.

## Plan review receipt

Independent source/adversarial, caller and full-map plan reviews are CLEAN after
clarifying Background versus command validation, typed failure preservation,
retry-evidence gating, fixed-clock Background retirement and the two native
envelope codecs. Exact inventory and append-only index checks pass. Neither
original RED execution nor production adoption is claimed. Actual #1923 is an
explicit prerequisite; both source owners cleared the selected regions.

## Current source-branch refresh

Branch feat/rust-bitunix-public-managed-reads starts from actual
`9447ee542cb7af2a62d30794d4ffc093a66720d5`, after actual #1923
`f489a0a0a3775d30bc35aa6c9552dca1bf75be45` and the independent #1903 prune.
The target public lib/account code is unchanged from the frozen inventory pin;
current totals are18 occurrences /16 rows /three files, and this slice removes
exactly eight occurrences /seven rows to leave10 /nine /two. Actual #1923 now
satisfies the public-auth-health prerequisite; no local suppression is added.

A fresh open-PR file map has no Bitunix source/test collision; both prior source
owners explicitly cleared these regions. The overseer approved this branch and
18->10 scope before any test/source coding. WebSocket behavior stays legacy,
including its existing Supervisor; no managed-socket migration or test-only
engine shortcut is included. Original assertion REDs are next, on this pinned
production, before any runtime change; historical plan review receipts do not
claim execution on the new pin. #1865 and #1922 heads remain frozen.


## Executed adoption and reviewed coverage

Original production on `9447ee542cb` compiled the separately committed tests:
`fda4818c41f` produced10 semantic failures and five compatibility passes;
`c2aaedb5431` expanded this to14 semantic failures and five compatibility passes.
The production cutover follows those commits. Two previously unreached expectations
were corrected against the shared contract: invalid clock is InvalidInput, and
unsent preparation expiry carries NotSent. The auth no-send assertion permits
resource cancellation while still forbidding Prepare, Bind or HTTP. None changes
the original failing preparation/delivery assertions or native wire expectations.

Markets and tickers now hold genuine Background Reads through validated staging
and Store publication, retire before follow-up/retry, and use managed unsigned
GETs with static native headers. FetchCandles admits its original Read before
interval validation and completes only through that root. Typed engine failure
hooks preserve supplied fields; failed/unproven responses use one BITUNIX table
and shared classifier retry evidence. Code-zero success messages cannot replace
decoder failures; malformed streams retain safe status/header evidence without
raw body previews. Existing native fixtures drive actual Prepare/Bind/Claim at
their event times; private signing probes seed Store directly.

The scanner confirms the exact selected eight-occurrence/seven-row removal:
18 occurrences /16 rows /three files become10 /nine /two. Signed dispatch and
private trading remain legacy debt. The socket implementation, signing helpers,
credential setup and conservative macro matches remain unchanged.

The overseer explicitly resolved the queue-capacity coverage question: record
that return branch as reviewed defense backed by shared-engine coverage, not an
executed native receipt. The native context registry and managed queue both cap
at256; genuine next-root admission dominates a full queue here. No fabricated
Context, engine injection seam or new local registry is added. Native entry
capacity, preparation refusal, transport failure, expiry and fixed-clock seed
retirement are executed, including filling all256 root slots after recovery.
The proposed overlapping retry-timer case is not a native issuer: markets starts
once then only from consumed retry timers; tickers starts once after markets,
then only from consumed retry timers. The overseer accepted this analysis and
required no local stop state. Future fresh initialization remains caller-owned.
