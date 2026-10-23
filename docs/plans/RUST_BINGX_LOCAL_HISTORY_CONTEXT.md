# BingX local history terminals

Owner codex-uufub4. Plan-first slice on actual landing 15bd7821b52e378a9e32a60ecb897275c52109a4. Four
synchronous branches only in history.rs::query; no remote history adoption.

## Scope and native behavior

1. Keyless private Fills/OrderHistory/Income return exact [] before private
   query validation. PublicTrades and unsupported client lookup retain their
   separate native dispatch ordering.
2. OrderByClientId refuses with its exact existing unsupported message.
3. Credentialed Fills without a symbol returns exact [] without HTTP.
4. An invalid/nonfinite/reversed normalized history range refuses with the
   existing invalid history time range message.

Each branch admits a genuine initial Read with credential None, timeout 20000,
pacing None and RetryPolicy::Never immediately before its local terminal. This
is truthful zero-I/O authority: no transport credential/route/signature exists.
Rejected/Revoked entry returns directly; successful issuance settles the original
Context exactly once through done_context/fail_context. Read failures carry no
financial outcome. Shared admission Capacity/Invalid errors remain typed; no
numeric fallback, synthetic Context, credential selection or remote root.

Native branch order, local JSON/messages and same-ID reuse remain. No Store,
provider, HTTP or timer effect is introduced. Existing remote fallthrough obtains
no new Context here and keeps its bytes, pacing, page budgets, callbacks and
legacy finalizer until the separately owned history/Plan conversion. All lib.rs,
account.rs, trading.rs, sign/managed recipes, metadata, session and engine files
stay byte-identical to the actual parent. #1954/#1934/#1747 held regions are not
edited. Remote history owner has been asked to flag any local-query hold.

## Original controls and implementation gates

Commit this plan and compiling native controls before runtime. Use real Bingx
and the test-support trusted provider/Prepare/Bind/Claim driver where transport
is used to occupy capacity. Finite native no-I/O replies and errors are working
controls. Invalid clock and full Context capacity must produce one exact typed
Read refusal without publication or unrelated owner retirement. Existing numeric
terminals are expected assertion REDs; missing symbols/compile errors are not RED.
Reuse a completed local ID, preserve credential-free native precedence, and
compare untouched remote history tests after adoption. No ignored new controls.

Independent plan/control review precedes source. Implement one local terminal
helper using existing genuine admission and Context APIs, with no ownership copy
or new engine seam. Exact expected shrink: history.rs 7F/8O to 4F/4O (one shared
[] fingerprint with two occurrences plus two failure fingerprints removed).
Prove the exact map, keep every unowned manifest key and remote source unchanged,
run focused new/native history targets, strict package Clippy, fmt/bans/scanners,
then independent source/caller/map review, draft PR and full exact-head CI.
No fixture/reference/budget/allowance edits and no merge without owner GO.

## Executed original receipt

On unchanged 15bd7821b52 the native target compiled and ran: 1 compatibility
control GREEN, 4 assertion REDs, 0 ignored. Full-registry setup released 256 real
managed FetchAccount reads through Prepare/Sign/Bind/Claim. The local branches
then returned their numeric native result/error instead of exact Capacity; NaN
entry time still published []. The finite native message/precedence/reuse control
passed. Production, manifest and fixtures are unchanged at this checkpoint.


## Implementation receipt

Independent plan/control review CLEAR at 1b242ee416c preceded source. One local
helper now admits and settles exact Read Context authority at the four exits.
The unchanged original target is 5 GREEN, 0 ignored; all 20 native history tests
also pass. The strengthened capacity control completes an earlier managed
identity owner after refusal and then reuses the local command ID successfully.
Remote methods/codecs from History::next onward are byte-identical to the parent.
Exact history inventory is 4F/4O; every other manifest key remains unchanged.
