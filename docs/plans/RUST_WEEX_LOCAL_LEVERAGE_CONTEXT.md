# WEEX local leverage refusal custody

Status: plan before compiling original assertions; production held.
Base: actual `557de7ae7fbf6d2a00fdd3cbf1d0a962bd7fc570`.

## Scope and ownership

Only three synchronous FetchLeverage refusals in WEEX lib.rs are proposed:

| Refusal | Fingerprint | Count |
| --- | --- | --- |
| unknown market | `1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43` | 1 |
| paused account reads | `6f0b47296411f75eb3a85b182bf43c08378b387a1a584d8848b0e811f29b01f9` | 1 |
| missing read credentials | `ab3206b428125b95aea7df47c71c9bfb8cef1809ff9c4432ba4ab8f34db5bac9` | 1 |

Do not change the existing SelectedRead::select call, Slots, read role, credential
policy or any key-selection site. The final numeric fail after managed::read
returns an error is a separate lifecycle site and is excluded. The valid signed
symbolConfig read, callbacks, codecs, response publication and metadata revision
checks remain byte-identical. No engine, account.rs, trading.rs or signer edits.

Combo-5b's three-dot source diff from this base names only BingX account.rs among
BingX/WEEX. #1747 (64e9f686d59) changes WEEX account.rs/trading.rs, not lib.rs.
This slice never starts or finishes its metadata writes, reads or generation
fences: every chosen branch returns before settings_revision and managed::read.
The missing-credentials branch follows the existing pure Selection lookup;
changing its completion does not change that lookup or its policy. The valid
path's interaction with #1747 remains native and outside this slice.

#2003, #2035 and #2042 also edit WEEX lib.rs. Preserve their actual landings by
plain merge when ordered; do not pre-resolve their runtime or change these frozen
heads. Shared docs/inventory compose mechanically. No inventory is changed by
this plan/original-evidence checkpoint.

## Native behavior

Keep exact validation order and complete native VenueErrors, all with absent
kind/outcome and other optional fields:

1. Unknown market: `unknown market {symbol}`.
2. While now < paused_until: `WEEX account reads are rate limited`.
3. If the existing Read selection fails: `WEEX leverage requires credentials`.

Unknown market wins while paused and without credentials. A live pause wins over
missing credentials. Equality with paused_until is not paused. Secondary-only
Selection remains allowed natively; this slice adds no primary-presence gate.
A non-finite now does not satisfy the pause comparison, so invalid-clock tests
must not pretend to reach the paused branch on that clock.

On an admissible entry each branch emits one failure and no transport, signer,
timer, auth/deactivation notice, Store mutation or subscription work. Preserve
settings and unrelated owners, immediate ID reuse, and the shared pause. Do not
publish fetched settings or borrow #1747's metadata ownership on a local failure.

## Existing authority only

After a native refusal is selected, invoke a private local helper. Use the shared
command_wait receipt gate: None uses reject_unstamped_command(Read), Expired uses
reject_expired_command(Read), and Remaining proceeds. No new default receipt,
numeric fallback, retained/public handle, Background or Plan is permitted.

Admit an ephemeral credential-free command Read Context with Never, no pacing,
and timeout min(20,000, remaining); immediately fail_context with the full native
error. Do not construct ReadCompletion, whose string error projection would add
Internal. No physical request or timer is created. Admission failure already
emits its canonical typed error; emit no additional native failure afterwards.
The helper is reachable only from the three genuine synchronous command branches.

Capacity or invalid receipt now supersedes the selected local error. An invalid
or nonadvancing receipt clock gives Revoked / `Request context was revoked`,
InvalidInput, outcome None. An expired own receipt uses the sole existing Read
Timeout helper. Record these deltas in RUST_CUTOVER_CALLERS.md at conversion.
Pre-existing native messages and absent fields remain for admissible entries.

## Original compiling assertions

Add tests/local_leverage_context.rs only, plus this plan/index. The actual WEEX
FetchLeverage dispatch must be delegated unchanged through real Engine entries;
no substitute local-error adapter, missing-method RED or production test hook.

Use a transparent fixture wrapper, as existing WEEX request_context_identity and
request_context_local_history controls do, solely to retain genuine Read roots
for capacity on both credentialed and keyless engines. Fixture-only sentinel
commands call admit_http_command and keep Contexts; every tested FetchLeverage
and native response is delegated directly to WEEX. The roots are real registry
owners, not physical HTTP requests; do not claim native transport proof for them.
A real overflow admission supplies the full canonical Read capacity error.

Expected REDs: all three native refusals under 256 occupied owners; unknown
market and missing credentials at a NaN clock. Every variant is a separate test.
Create a native pause via an actual unsigned order-book 429 with Retry-After,
not by mutating private state. Keep all fixture event times monotonic.

GREEN controls pin exact full errors and native precedence, keyless/private
parity where reachable, settings unchanged, ID reuse, genuine capacity refusal
and peer retirement/recovery, duplicate ownership, pause equality, and a valid
managed signed leverage read. The remote positive is preservation evidence for
the unchanged path, not a request-site conversion. Shared failure and deferred
callback custody remain separate source/API gates where not directly exercised.

Run only this changed target with CARGO_BUILD_JOBS=4, then scoped Clippy/fmt.
No local full-workspace/all-feature sweeps. Get bounded native/core review of the
plan and original evidence before production. This task authorizes plan + REDs
only; do not push a PR or change runtime/inventories at this checkpoint.
