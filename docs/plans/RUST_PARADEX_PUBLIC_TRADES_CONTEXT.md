# Paradex public trades: Context history consumer

## Baseline, authority and dependencies

Plan-only branch `feat/rust-paradex-public-trades-context` starts at the exact
reviewed #1848 pin `3554aef6a02aa960164e591c61134d8fd4e7653d`, explicitly
authorized by claude-ddi2il. It contains the reviewed shared history bridge and
command-entry/root-validation prerequisite. No engine source belongs to this PR.
Reintegrate actual #1830/#1848 landings before final consumer proof.

Source begins only after the held phase-1 Paradex files are available. Prefer
actual #1813 (book read dispatch, refreshed head `106f90eccb4`) and #1829 (public
startup reads and prepared-action test support, head `b82b453c398`). The overseer
allows an exact frozen own-stack if these are several hours away, including only
the narrow public-history typed forwarding in the lane's lib.rs region. Until
that choice is made, neither held file is edited. Original history.rs and
history tests are unheld in the open-PR file map checked before this plan.

## Existing native contract and inventory

`history.rs::History::query` accepts `Query::PublicTrades` for both public-only
and private-enabled options. The native request is unsigned GET
`{account.rest()}/trades?market={encoded market.id}&page_size={limit}` with empty
headers/body and `ArrayAt::Field("results")`. The default limit is 100; the wire
page size and output cap clamp the caller's limit to 1..500. Missing markets
complete with `[]` without HTTP or authentication. Preserve mainnet/testnet
routing and native market lookup/encoding exactly.

This is already one physical page: `on_response` forces `next=None` for every
PublicTrades response, even when a continuation exists. Do not follow a cursor,
change native completeness policy or introduce a Plan. Lighter's unsigned trades
are not a substitute: they can page by next_cursor and remain Plan-gated.

Native row validation precedes symbol filtering/dedupe. Preserve required fields,
finite numeric checks, BUY/SELL decoding, ignored foreign symbols, first identity
wins, and failure of the entire page on a malformed row or envelope. Reuse the
existing native Row/valid_row/Output::Trade codec; do not fork a second decoder.
PublicTrade output retains id, symbol, price, qty, quoteQty=price*qty, side and
millisecond timestamp. Preserve native response order and truncate after dedupe;
public trades are deliberately not sorted like private histories. No Store,
account, ticker or live trade publication is part of this history command.

The full Paradex legacy inventory and later private/history dependencies are in
#1813's RUST_PARADEX_REQUEST_CONTEXT.md. This slice moves only the public-trades
runtime path out of numeric Completion/CursorHistory. Shared legacy private
history sites remain counted until their own migration. The public sender targeted for removal is history.rs fingerprint
`a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865`
(count 1). It must disappear rather than remain as an unreachable public sender.
Private query variants must retain their existing completion and request behavior.
The other 13 history occurrences remain counted. The expected local history map
is 14→13; after both phase-1 PRs land, overall Paradex is expected 56→55 if no
other landing change affects its map. Verify instead of assuming these totals.
After implementation, run the exact scanner, remove only lexical sites actually deleted from Paradex,
and record the resulting hashes/counts in this plan. Never prune a still-present
site or change another venue's manifest entry to make the checker pass.

## Implementation shape

Use `Pending::PublicTrades(history::public_trades::Request)` with a dedicated
`Box<ReadCursorHistory<Output>>`, exact managed leg, PublicTrades query and native
valid/raw-row facts. Its ReadCompletion retains the immutable original Context.
Reuse `CursorWalk::new(page_cap(&query))` and existing stall/encode functions; the
native forced next=None still prevents a second page.
Keep the existing private history Request and legacy authority separate and
unchanged. No runtime enum selecting legacy versus Read authority, generic local
finalizer, numeric fallback or venue admission/retry policy is permitted.

Place the public adapter under history.rs (a child module may reuse its private
native codecs). Route PublicTrades to it before constructing any private history
owner. Remove the obsolete public legacy sender where practical; retain any
shared private lexical site in the inventory. The new Pending variant and callback
routing live in lib.rs after its held dependency is available. Do not add native
HTTP capabilities, use a signer or send private requests through unsigned ingress.

Initial entry uses only `cx.admit_http_command(Policy)` with Operation::Read,
Retry::Never, no credential and one original 20,000ms raw deadline. On Rejected,
the engine has already emitted exactly one correlated denial; on Revoked return
without touching another owner. Allocate no HTTP/timer/signing resource before
successful admission. Construct `ReadCompletion::from_context(cx,command,root)`;
handle denial without numeric completion. Check market before constructing the
ReadCursorHistory: on a missing market consume ReadCompletion directly through
`.finish(cx, Ok("[]".into()))`. Only a real HTTP page constructs ReadCursorHistory.
A fresh cursor owner cannot finish successfully without admitted advancement;
never manufacture an empty page, guard or delivery to bypass that constraint.

For a physical page, use managed GET preparation and the original root/leg.
On successful `request_managed`, call `cx.stream_response(id,
ArrayAt::Field("results"))`; preserve native envelope validation. Gate every row/dedupe mutation through the borrowed `.admit(cx,&leg)`
guard; it binds the exact physical Attempt as well as issuer/owner/generation.
Drop the guard before mutable Cx diagnostics or terminals and re-admit if needed.
On a valid final envelope call the guard
`advance(CursorPage { next: None, rows: raw_rows, fresh: raw_rows, want_more: true })`,
preserving native forced single-page advancement and ignoring its cursor as before. On a malformed/failed prefix discard unvalidated staging
before strict rejection; never return accepted-looking prefix data.

Use the shared Read terminal, including `finish_with` for the native truncate and
encoder. Handle/ignore ContextError as revoked ownership, never unwrap or retry
numerically. Typed preparation and released failures must reach the public owner
without the existing default string bridge losing their kind/outcome. Forward
only this Pending variant in lib.rs; preserve all existing private paths. A stale
or reused owner emits neither a result nor history.incomplete. No local marker,
status ladder, auth inference, retry, timer or completion implementation is added.

## RED first, validation and independent review

Commit assertion-failing tests before source. Use the actual Engine managed
Prepare/Bind/final-claim path and shared test_support; no mock Context, API stub,
manual legacy fallback or duplicated generic transport fixture. Existing public
history controls remain intact and green after migration.

Controls must prove:

- Original unsigned GET route in both public-only and private-enabled modes,
  with empty headers/no credential, body, default/0/1/500/above-500 limits,
  native result schema, symbol encoding, order, identity dedupe and truncation.
- Preparation parks the request; response before final claim cannot publish;
  successful claim/release completes once. A response cursor never sends a page 2.
- Missing-market completion has no I/O and uses admitted command authority.
- Native status/envelope failure, malformed middle row and broken streamed body
  reject without leaking a valid prefix, marker or Store/account mutation.
- Typed preparation denial and policy/capacity refusal produce one correlated
  terminal with their shared metadata, no legacy send or numeric fallback.
- Original deadline before/after release and command-ID reuse fence old response,
  old preparation token and old completion without disturbing the replacement.

Obtain independent source/adversarial, caller and full-map plan reviews before
source. Repeat all three after implementation; state the architecture-gate answer
in the PR. Expected answer is No: a native single-page protocol adapter consumes
shared entry, transport, admission and history terminal choke points.

Run focused Paradex and Binance native tests with CARGO_BUILD_JOBS=2, touched-crate
strict clippy/fmt, bans and both request-context checks. Use CI for WASM, replays,
size and timing. Supply the real venue consumer receipt on the exact authorized
stack to the history owner; engine-only fixtures do not satisfy it. Keep CI green,
request the owner's merge window and run a separate fresh full-map revert guard
before any head-matched merge.

## Callers

`RustEngineExchange.fetchPublicTrades` retains its arguments, native data shape/order/limits and
single-page behavior. Custom worker/hosted transports need prepareHttp for this
read. Released transport loss retains shared Network classification and engine
expiry retains Timeout instead of the legacy Internal fallback. Native malformed
row/envelope or HTTP status-text errors retain their existing Internal projection;
no local status ladder is added. Preparation denial and expiration use engine
authority and typed error propagation; an expired or reused ID cannot complete from an old page. No caller
signature, new REST capability or private auth behavior changes. Record concrete
landed behavior in RUST_CUTOVER_CALLERS.md, RUST_AUDITS.md and RUST_QUALITY.md with
the implementation; this plan alone claims no source cutover or consumer proof.

## Current reviewed-stack integration after callback release

Callback dependency #1813 actually landed3c4c3c3d042; seeds #1829 are also actual.
This branch genuinely merges reviewed #1848 f4719c16597a3fd852f0ebf68a685a733802a49a
and actual #1871/#1872 landing e233cb05dfb. All engine source/test blobs are exact
to the authorized f471 bridge; no core conflict or local seam change occurred.
The earlier three CLEAN plan reviews and seven original RED controls retain the
same API/native contracts. Shared README/CALLERS merges preserve full incoming,
bridge row/appendix once and the exact own public-trades index row once. No
Paradex signing, auth, account/trading or private history authority is changed.
Sam's pending remote Stark payloadSigner option C has no bearing on unsigned
GET /trades. The current source/test hold check clears history.rs/lib.rs and the
new regression file; recheck before source. No reviewed seam is claimed actual.

## Executed native consumer: seven original controls now GREEN

On current reviewed stack0e2d36075ae, all seven original efafc26dedc controls
compile and fail by assertion on the original native legacy HTTP dispatch.
Implementation puts public trades in history/public_trades.rs with a dedicated
boxed ReadCursorHistory, retaining the existing Row/valid_row/native trade codec
and keeping the legacy private owner separate. The original Read root is also
the one physical leg; every row identity/stage/advance uses borrowed admission.
Missing-market [] completes directly through admitted ReadCompletion. A failed
page discards staging and strictly rejects; it cannot emit a partial marker.

All seven controls pass unchanged after implementation, executing real
Prepare→Bind→Claim→response, both network/private-enabled combinations, encoded
market and limit clamps, raw native order/dedupe/truncate/schema, ignored valid
cursor and rejected malformed cursor, failed prefixes, typed route/transport
failures, parked/released expiry, capacity before empty result and stale reuse.
Shared engine source/tests remain byte-exact reviewed f471; no Plan/Execution or
signer/session authority is cast into a Read root. Typed forwarding is narrow to
the new Pending variant; existing book/private error paths remain unchanged.
The obsolete public request_streamed is deleted; the actual scanner removes only
a876e5300951dc42d8c9828d78f0d32d52e42b439cbaf763cc6cdd6b63174865 once:
Paradex56→55, history14→13. Full native/strict checks, independent implementation
reviews and CI remain required; this is native consumer evidence, not actual
bridge landing or a whole-venue readiness claim.

Frozen source38836aad176 passes501 Paradex+Binance native tests across41 result
groups, including the seven unchanged original controls, strict Paradex all-target/
all-feature Clippy and scoped fmt. Source/adversarial, caller and full-map reviews
are CLEAN (architecture gate No). Genuine actual2badd1cb0e3 integration changes
only incoming other-venue source/docs/inventory; all Paradex/Binance/engine/crypto
blobs remain exact to the tested source. Scanner, its14 controls and bans pass on
the integrated tree. Full bridge source/tests still match authorized f471 exactly.
This supplies the second real unsigned native Read-history consumer receipt;
CI/actual bridge integration/merge window remain separate gates.
