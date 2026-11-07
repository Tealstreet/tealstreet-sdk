# WEEX local leverage refusal custody

Status: production conversion and scoped controls; draft source review and CI pending.
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

## Original evidence receipt

Plan commit `0336a336afc` precedes the new test target. Executed with the native
WEEX source and engine byte-identical to the pinned actual base:

```sh
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2 cargo test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-weex --test local_leverage_context
CARGO_BUILD_JOBS=4 cargo clippy --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-weex --test local_leverage_context -- -D warnings
```

Result: five independent assertion REDs, six GREEN controls, zero ignored tests.
All REDs reach full-error comparisons, with no compile, setup, missing-method or
missing-request failure. Capacity mismatches preserve the native local error
where a real overflow admission produced canonical Read Capacity; NaN clock
mismatches preserve the native error instead of canonical Read Revoked.
The 256 retained owners are real Contexts from genuine fixture entries, without
physical requests; no 256-request native wire claim is made.

The paused-capacity case first receives an actual unsigned order-book 429 with
Retry-After 3, then retains the owners at a live finite clock. No private pause
mutation is used. Native controls verify unknown market over pause/credentials,
pause over credentials, equality at 4001, exact absent error fields, settings
unchanged, repeated local ID reuse, and live native signed leverage wire/output.
The independent capacity GREEN executes overflow refusal, duplicate diagnostic,
exact peer retirement and a new owner taking the freed slot.

The three capacity REDs stop at their error mismatch. Their later settings,
local retry and second-peer retirement assertions remain explicit post-cutover
execution gates. Shared expired/deferred-callback behavior and managed protocol
counts are not new native injection proofs in this checkpoint. Scoped target
Clippy and rustfmt pass; no broad build or CI dispatch was run. Production,
engine, maps, checker, selection, account and trading source remain base-exact.

## Production conversion and scoped receipt

Both native and core bounded plan/original-evidence reviews were CLEAR. The core
review independently reproduced five assertion REDs and six GREENs from exact
`0487af668c0`; native review was read-only. The overseer then authorized runtime.
The branch plainly integrates actual BookQuery landing `46cb0941346`; runtime
merges automatically. FetchLeverage's Selection call and valid signed suffix
remain byte-identical to that incoming source.

The conversion adds one private refuse_leverage_query helper and replaces only
the three numeric local failures. The source-derived inventory diff removes the
three named digests only, without key/foreign inventory changes. CALLERS names
the cause-aware receipt and capacity precedence, preserving native absent
metadata and the immediate pause refusal. No shared engine/API, account,
trading, signing or #1747 metadata ownership change is introduced.

All eleven original test bodies remain an exact file prefix. They now pass,
including each capacity recovery suffix. Five appended controls add private and
keyless NaN refusal followed by a finite same-ID local re-ask, and primary plus
secondary-only exact native signature-oracle and published-setting assertions.
The local target passes 16/16; the existing selected_signed_read_wire target
passes 1/1 across its key configurations. Tests use seeded setup without Start;
no full startup or private-socket handshake proof is claimed.

Scoped strict Clippy, rustfmt and bans/ratchets pass before pushing.
Native/core source review and full CI remain separate readiness gates. Missing
or expired deferred callback coverage remains shared API/source evidence: the
private helper has only the three genuine synchronous command callers.
