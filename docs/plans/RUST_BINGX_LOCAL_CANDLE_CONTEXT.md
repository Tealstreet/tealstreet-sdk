# BingX local candle refusal custody

Status: plan and compiling original assertion evidence; production held.
Base: actual `db6fa45bbb7cbbfdfeb0aa40d3dd93caac9c9e0b`.

## Scope and ownership

Only the three synchronous FetchCandles refusals in
`sdk-rs/venues/bingx/src/lib.rs` are proposed. The command's valid signed/public
request paths, callbacks, streaming, re-sign, codecs and outputs stay outside
this slice. No account, trading, identity, history or key-selection source edits.

The prospective source file is disjoint from the pinned combo-5b file set:
`git diff --name-only db6fa45bbb7...train/combo-5b` names only BingX account.rs
among BingX/WEEX sources. Keep account.rs and every Selection/Slots/credential
site unchanged until 5b lands. Shared docs/index and exact inventories can be
composed mechanically; this checkpoint does not edit any inventory or source.

Exactly three occurrences, all in BingX lib.rs:

| Refusal | Fingerprint | Count |
| --- | --- | --- |
| unknown market | `1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43` | 1 |
| unsupported REST interval | `ca1a695870588abe078efa57b2d1e6f789b219044369ce2f8d1119a9c10664b7` | 1 |
| invalid candle time range | `b4b475ef7a3c08d3514bf2b7497dc3a85e7a328c33757287a83f82cb602f0a27` | 1 |

Shared request issue sites are not counted as removed. No STAGING/checker or
credential/signing inventory changes are proposed. At implementation, derive
the three-occurrence request-inventory prune from source, preserving other keys.

## Native behavior to retain

Validation order is unknown market, unsupported interval, then invalid range,
all before credential selection. Keep each complete native VenueError: exact
message with absent kind/outcome/venue fields. These are local Read refusals,
not delivered Internal failures or Mutation NotSent errors. Do not route through
ReadCompletion's string-error normalization.

The range is computed natively: end defaults to now; with no limit, start defaults
to end minus 500 intervals unless from_ms is supplied. With a limit, start is
end minus step times min(limit, 1500), even when from_ms is supplied. Refuse only
nonfinite start/end or start greater than end. Preserve these calculations;
no new validation, bounds, filtering, market normalization or credential gate.

On genuine admissible entries, each local refusal emits exactly one failure,
no HTTP, preparation, signer, timer, Store publication or subscription effect.
No credential lookup or auth/deactivation evidence accompanies these branches.
Immediate command-ID reuse must work and unrelated live owners must survive.

## Landed authority only

Use the existing Read entry/admission transaction, as the landed local history
refusals do. A small local helper is invoked only once a native refusal has
already been selected; valid remote candle commands never pass through it.

Guard the genuine receipt with command_wait: missing receipt calls the shared
reject_unstamped_command(Read), Expired calls reject_expired_command(Read), and
only Remaining continues. No caller ID lookup, numeric fallback, fresh default
receipt, retained handle, Background or Plan is permitted. Borrowed/spent entry
refusals stay effect-free under the shared API's ownership rules.

Admit an ephemeral Read command Context with no credential, no pacing, Never,
and timeout min(20,000 ms, remaining). This is immediate local completion, not a
physical request: no HTTP or engine transport timer is created. Finish the exact
Context through fail_context with the selected native error, preserving its
untyped shape. Admission refusal returns through its existing typed transaction;
never emit the native failure afterwards. No new shared engine API is needed.

The local owner must never be confused with the already-managed remote candle
Context. Do not spend Preflight, join metadata groups, borrow account ownership
or move any existing selection/signing calls. Capacity/invalid entry takes
precedence over the selected native error; this is the caller-visible change
to document in RUST_CUTOVER_CALLERS.md with the production conversion.

## Original compiling assertions

New test file only: `sdk-rs/venues/bingx/tests/local_candle_context.rs`. Use the
actual Bingx engine, injected native fixture signer, and public Store market
setup. External Event::Command supplies a genuine command entry. Do not add an
implementation-shaped missing-method test or a substitute venue adapter.

Expected independent REDs: each of the three branches under genuine full owner
capacity; each at a nonfinite clock. Fill capacity with 256 real managed
FetchAccount owners using prepared_actions. An additional genuine overflow
FetchAccount supplies the exact canonical Read capacity error. Compare that full
value, not a literal capacity string. Assert one terminal and no transport or
Store effects. Every variant is a separate test, so an early RED hides no other
claimed variant.

GREEN controls pin the native messages, absent fields, validation precedence,
keyless local refusal, immediate ID reuse, successful original identity owner
after an overflow refusal, and duplicate-ID preservation. Values are fixture
placeholders. Test setup may execute existing key selection but changes none
of its source, policy or sites.

Before readiness, add resource/receipt boundary and no-double-terminal controls
as needed by the exact source, retain native valid candle wire tests, obtain
bounded native/core review, and regenerate only the actual own inventory prune.
Build only this changed target with CARGO_BUILD_JOBS=4, serially; CI owns broad
feature/workspace/CPU/WASM checks. No production code or PR is authorized by this
plan checkpoint alone.

## Original evidence receipt

The plan was committed first at `152dc3191e5`. The original target executes the
actual BingX venue on the unchanged `db6fa45bbb7` engine and production source:

```sh
CARGO_BUILD_JOBS=4 RUST_TEST_THREADS=2 cargo test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-bingx --test local_candle_context
CARGO_BUILD_JOBS=4 cargo clippy --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-bingx --test local_candle_context -- -D warnings
```

Executed result: six independent assertion REDs and five GREEN controls, with
zero ignored tests. The three capacity REDs first release 256 genuine managed
FetchAccount requests and capture the full error of a real overflow account
command. Each local branch then incorrectly emits its native untyped error
instead of that canonical capacity error. The three invalid-clock REDs likewise
emit the native error instead of the canonical Revoked Read error with
InvalidInput and no outcome. All REDs reach exact full-error comparisons;
there is no compile, setup, missing-method or missing-request failure.

The five GREEN controls preserve each native full error and validation order,
keyless/private parity, immediate repeated ID reuse, real overflow refusal plus
original-owner completion and freed-slot reuse, and duplicate live-ID ownership.
Each local failure helper permits exactly the correlated failure notice, so
transport, timer, signer, auth and other notices cannot hide behind it.

The original capacity REDs stop at their expected error mismatch. Their suffix
that completes the original owner and retries the local command remains an
explicit post-cutover gate; the separate genuine-capacity GREEN executes peer
preservation and slot reuse today. No expiry/deferred-entry execution, native
remote candle adoption, production conversion, inventory prune or readiness is
claimed by this evidence. Scoped target Clippy and rustfmt pass; no broad local
build or CI dispatch was run for this checkpoint.
