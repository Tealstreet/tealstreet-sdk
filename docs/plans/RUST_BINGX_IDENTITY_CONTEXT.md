# BingX FetchAccount selected Read custody

Plan-first, 2026-10-24. Owner codex-zs5e83; explicit handoff from codex-uufub4.
Branch `feat/rust-bingx-identity-context` starts at actual `64f4faa202d804d6fc9cf84f314e5f581df10105`.
The overseer was notified before coding. Initial plan/index commit
`2119b0ec9df` preceded original assertion RED commit `fa95669213c`; production
and inventory remain unchanged in both checkpoints.

## Scope and collision boundary

Full single-request FetchAccount identity Read: local configuration/keyless
refusals and the existing signed GET `/openApi/swap/v3/user/balance`.
Production is held until actual #1962 lands. Its reviewed frozen API is
`62d2db98b2fa9e6d00723dd1b17df4ebce370eee`; inspect that pin, do not copy or
rebuild `managed.rs` or synthesize a Context/credential while waiting.

uufub4 confirmed identity.rs is unstarted/unheld. This lane may change only:

- `sdk-rs/venues/bingx/src/identity.rs` and a NEW dedicated
  `tests/request_context_identity.rs` (existing identity/managed fixtures held).
- Narrow lib.rs hunks: Pending::Identity representation; FetchAccount dispatch;
  identity-only DATA/failure/preparation branches, including Signed wrapping;
  identity-only stream no-op arm if needed. All other Pending arms and generic
  Signed/candle/helper bodies stay exact. Send this hunk list before source and
  coordinate integration with held #1954 and #1747/#1884 owners.
- Shared plan/ledgers/index and the exact identity manifest prune in adoption.

No Trade, account/metadata, history, socket/session, signer, provider, shared
engine or generic read-helper changes. The open-PR scan at this base finds
#1962's new selected_read_entry.rs held, #1891's inherited identity/managed tests,
and #1884/#1747 lib/account/metadata claims. NEW plan/test paths are clear.
Do not edit those existing fixtures or any shared source as part of this slice.
The old helper's now-unreachable identity special case is owned by its shared
adoption owner; any cleanup outside identity-only lib hunks needs coordination.

## Native contracts and observed gaps

Source anchors at 64f4faa202d804d6fc9cf84f314e5f581df10105:
identity.rs:1-171 (codec/entry/terminals), lib.rs Pending/Signed and identity-only
on_response/on_sign_error/on_request_failed/FetchAccount arms; sign/dispatch.rs
Http and its unsigned headers. Existing tests/identity.rs and managed_reads.rs
are retained controls, not changed by this checkpoint.

- `config_error` wins before the absent-primary gate, including an invalid saved
  secondary in public-only/keyless configurations. Preserve exact native text.
- Public-only or absent/invalid primary refuses, even if a secondary is saved.
  No public unsigned fallback exists for FetchAccount.
- Ordinary Read selects Secondary when configured, else Primary. Primary must
  exist. The original helper infers this from GET; the adoption declares Read.
  Identity carries no account-auth evidence scope and must not publish account
  state, trigger reconciliation or mark auth health.
- GET URL/query/body remain native. Empty params become `timestamp=<integer>`
  plus the native HMAC signature at dispatch. Unsigned Prepare headers remain
  ordered X-SOURCE-KEY: Tealstreet then Content-Type:
  application/json, charset=utf-8. Provider presents/signs exactly the selected
  opaque refs. Native dispatch recipe remains unchanged.
- One original 20,000ms Read root, one attempt, RetryPolicy::Never. In particular
  100001/signature rejection never retries identity. No Plan/session/new deadline
  or root renewal. Physical preparation/provider/release wait consumes the root.
- Parse exact non-2xx/code/msg/data errors; USDT row wins, else first row; ID is
  nonempty text or exact u64 decimal (including beyond JS safe integer). No
  account Store write. Existing successful terminal is already Context-bearing.
- Gaps verified in source: local finish uses numeric done/fail; managed::read
  converts entry refusal into generic Internal/NotSent; default on_http_failed
  discards typed Network/Timeout before identity's terminal. These are separate
  assertions, not a claim that current remote success or duplicate handling fail.

## Frozen shared API and proposed adaptation

Consume `safe_cex_engine::selected_read::{SelectedRead, AdmittedRead, ReadRole,
ReadPolicy, ReadRequest, LocalRefusal, CommandReadError}` exactly as #1962.

1. Declare eligible opaque slots with existing Slots/SlotPolicy
   (secondary_requires_primary=true), preserving BingX's configuration/public
   gates. A native configuration refusal supplies no usable selection and its
   exact LocalRefusal diagnostic/error; never send a malformed identity to a
   recipe or fall back to Primary. This eligibility projection is subject to the
   helper owner's plan review, not a second selection/admission implementation.
2. `SelectedRead::select(..., ReadRole::Read { scope: None })` and
   `SelectedRead::command(cx, selected, ReadPolicy { timeout_ms: 20_000,
   pacing: None, retry: Never }, LocalRefusal { diagnostic, error })`.
   Entry refusal already emitted the engine's single denial: return without a
   numeric failure. LocalRefusal already handles checked diagnostic+terminal.
3. Pending::Identity owns the returned AdmittedRead. `AdmittedRead::queue`
   receives unchanged ReadRequest method/URL/body and no stream; its recipe
   closure builds the existing Http from the exact selected Identity, and the
   pending closure carries the same AdmittedRead. No re-selection by method,
   independent Context field, local ownership wrapper or third helper.
4. Keep the native parse/serialization logic. Check exact original root/current
   delivery before any diagnostic/result; finish through context-bearing APIs.
   Typed queue/preparation/provider/network/deadline failures retain the genuine
   wrapper/root and exact VenueError, with checked companion diagnostic before
   the one terminal. Handle Context errors without unwrap or numeric fallback.
5. Only identity moves out of the generic Signed wrapper. Remove obsolete
   identity-only numeric finalizers and unreachable identity-only Signed arms;
   leave all generic Signed/candle/account/history behavior exact. Narrow
   on_http_failed forwarding preserves typed errors for Identity and delegates
   all non-identity cases to the previous behavior. No local status ladder.

Plan/slot/public egress extensions, command-wait geometry, session waits and
G1/G2 provenance are not prerequisites of this existing one-GET scope. Do not
claim those capabilities. The current native 20s physical/root shape is kept.

## Original REDs before production

NEW tests use only existing public engine APIs on unchanged 64f4faa202d804d6fc9cf84f314e5f581df10105 source;
no future-helper call or compilation failure counts as original RED evidence.
A delegating Probe obtains genuine retained Read owners via real command entry,
forwards typed HTTP/preparation/sign callbacks, and leaves all native FetchAccount
logic untouched. It records no provider secrets or fabricated authority.

Working controls pin local refusal ordering/metadata and precedence, public-only
and primary gate, exact remote primary/secondary refs/wire and u64/text codec,
unchanged account snapshot, duplicate entry/response, and no 100001 retry.

Intended original assertion REDs:

- At the genuine 256-root bound, keyless/public-only/invalid-secondary local
  refusals must be the shared single Capacity denial, not bypass admission.
- Remote entry Capacity keeps the engine's exact error/kind and no financial
  outcome, no generic helper Internal/NotSent fallback or extra diagnostic.
- Invalid raw clock is typed before local or remote work, without numeric fail.
- Released request transport failure and shared deadline expiry preserve typed
  Network/Timeout through the identity-only callback, with the native companion
  diagnostic and one terminal; stale/duplicate callbacks emit nothing.

Retain passing compatibility assertions as GREEN; never manufacture failures.
Recovery/refill and surviving peer controls use actual owner handles. Fixed-clock
local cycles must retire slots; duplicate numeric entry must not consume a peer.
Deeper assertions blocked behind an earlier RED are labelled unexecuted. Missing
API names or broken fixture setup are corrected and excluded from RED receipts.

## Inventory and verification gates

At 64f4faa202d804d6fc9cf84f314e5f581df10105, BingX is 34 fingerprints/37 occurrences. identity.rs is 2/2:
`c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548`
(done) and
`c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`
(fail). Exact proposed prune is identity2/2->0, family34/37->32/35 at this base.
lib remains8/9. No inventory entry changes in the plan/RED checkpoint; actual
landing counts must be recomputed when the adoption starts. Keep all incoming
foreign entries and reapply only the two proven identity removals.

Commit plan/index, obtain independent source/adversarial, caller and full-map
reviews plus helper-owner API review; then commit compiling original REDs.
After actual #1962 and renewed collision check/owner hunk release, implement
without weakening assertions, prune exactly the identity key, append CALLERS and
update ledgers. Scope gates: focused dedicated+native identity/managed tests,
BingX/Binance checks, strict touched-target Clippy/fmt, request-context scan and
its tests. CARGO_BUILD_JOBS=2. CI owns WASM/replay/size; no local release builds.
Independent implementation/caller/full-map reviews, one slow CI watcher, green
actual integration and an explicit train slot precede separate guard/merge.

Architecture gate: No per-venue implementation of shared selection, admission,
clock, retry or completion policy. This consumes #1962 and existing typed core
terminals; the venue retains its native codec/eligibility presentation only.

## Full pinned map

All entries outside the two identity terminals remain held by their existing
owners; no source or inventory movement is implied by listing them here.

| Path | Fingerprint | Occurrences | Disposition |
| --- | --- | --- | --- |
| `venues/bingx/src/account.rs` | `29e80488342dd1d485fc314630b169041c758cdb285d6713600ce2c41bdf9bca` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `4fd69fe1e61d7806268e180e1316430a4f1412b5b180eee8a9d78451ac57c7d9` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `55851e8e0638146955d1125db1ae3ef70da6a19fc91d90c46c2968a47192c2a6` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `5a3ecde1bdd5b82ea5d3f97b6da8fa145ee3d7db5f208cc0f5157b22ac098f3c` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `5c468cf227f077b2cd510a41debb723fda4e174f1fd78a78d58c5cdaa4aac543` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `8907f38dcee7367c011ac797732f8780004c6710d7395d59812584ac876a5ddc` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `9bab5c71c1e088436a8eadaf05e2474ba937539fb986b022edf43b0253dbb6d3` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `a47fe33aebc693aaebc6ecbdf190ec1a18170a36c47d087bfa086628e4d42c9d` | 2 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `ad0827b91e9a260b851c58b5f25e98d44a810f4cae2b83721c07cd4e85ec9899` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `b8981ba6ded29689d47b26a8a93c34eb801bdbb171693c719f0eef8a920e632c` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `ca85021da83ecbaa3f4b5752d5bb180f2400bda2b2b348bd02b7c57b833b424e` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `cb3ce1adb2967400edbc53b042baee6c43b02aca95322788d162e10e89d6f271` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/account.rs` | `f025fc43a750a2590ced1ab109e5ee504164bf8505fdfb4ffec28b246dacd511` | 2 | Held: existing owner; unchanged |
| `venues/bingx/src/history.rs` | `4ae3536eac29fa18dd9f9418fd828aa837bdf6c0fba82d503fa82b0539828990` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/history.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/history.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/history.rs` | `f43453f19ab26af8ad6edee4529387f502a76b04b80658aac68a19e1034c4904` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/identity.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | This slice: shared checked Read terminal |
| `venues/bingx/src/identity.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 1 | This slice: shared checked Read terminal |
| `venues/bingx/src/lib.rs` | `1620031f97e957d61d90d1f02aeed59230d68b71da7aea4f13fe7ae663dcdc43` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `b4b475ef7a3c08d3514bf2b7497dc3a85e7a328c33757287a83f82cb602f0a27` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa` | 2 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `ca1a695870588abe078efa57b2d1e6f789b219044369ce2f8d1119a9c10664b7` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/lib.rs` | `fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `74117de059064b62a6517c237b3df816c69d9bca5336e0c31c75ef137ee9e609` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `7fd80a6abee4977d950a002d01df9be16043d5c2bda90254c082f38174cb8180` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `b3a2cf7a716599d4c58dffccfa7f4ef44af90ea53d785e8ee13bf034aed3682e` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `b70e9c2e2d2a7b5bf31d71625ed006497f17176e855c618b929752eb3c7d6ab8` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `b9fe9d4e02015da4b49bb3e467c65f8fbec06c81605999dc39c08de9f077b18a` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `df1e843744fe35d63b22778691cfbe4de4aece149d94ad0c5343e03739d6c3df` | 1 | Held: existing owner; unchanged |
| `venues/bingx/src/trading.rs` | `fbafcc48aaedbe70a22d054c16acc0858b261af3d5f711681c39d0e866da7e30` | 1 | Held: existing owner; unchanged |

## Executed original assertion checkpoint

Test-only commit `fa95669213c` adds only
`sdk-rs/venues/bingx/tests/request_context_identity.rs`. Against unchanged actual
`64f4faa202d804d6fc9cf84f314e5f581df10105` production, the existing-API target
compiles and executes **6 PASS / 8 intended assertion REDs / 0 ignored**:

```sh
CARGO_BUILD_JOBS=2 cargo test --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-bingx --test request_context_identity --locked
CARGO_BUILD_JOBS=2 cargo clippy --manifest-path sdk-rs/Cargo.toml \
  -p safe-cex-venue-bingx --test request_context_identity --locked -- -D warnings
```

The test exits 101 because of the eight intended assertions. Strict scoped
Clippy, touched-file rustfmt check and git diff check pass. The compiling receipt
is `/tmp/codex-zs5e83-bingx-identity-original-reds.log`; Clippy receipt is
`/tmp/codex-zs5e83-bingx-identity-clippy.log`. These logs are not committed.

- Four local capacity REDs: missing primary, public-only, secondary-only and
  invalid secondary bypass shared admission and emit native diagnostic/failure.
- Remote capacity RED: generic lifecycle-unavailable diagnostic plus
  Internal/NotSent replaces the engine's exact Capacity denial with no outcome.
- Invalid clock RED: first keyless/NaN case emits native diagnostic/failure,
  instead of the engine's typed InvalidInput denial before local work.
- Released network and root-timer REDs: both reach the intended kind assertion,
  observing `(None, None)` instead of `(Some(Network/Timeout), None)`. Exact
  preparation cancellation, one diagnostic-before-terminal, stale late-response
  suppression and unchanged Store checks execute before that assertion.

The six GREEN tests pin all local eligibility/config precedence combinations,
fixed-clock local retirement and genuine peer reuse, exact real-provider primary
and secondary golden wire plus u64/text/USDT codec, opaque selected refs and
pending duplicate ownership, malformed/status/100001 refusal without retry, and
typed preparation/provider refusal with no HTTP release. Cancellation checks
permit only the tested exact preparation; provider refusal additionally pins its
known signing attempt. Pre-prepare signing IDs are engine-private.

Capacity recovery/refill suffixes and later clock cases are present but blocked
behind the first failing assertion; they are not claimed executed. Fixture
compile, cancellation-cleanup and snapshot-serialization-clock mistakes were
corrected and excluded from the RED evidence. Actual timer/late HTTP event
clocks remain unchanged; Store frames use the same serialization clock.

Independent source/adversarial and caller reviews are CLEAR after those fixes;
full-map review confirms only the two original docs plus this new test differ
from the base, with all production/held files and manifest unchanged. Architecture
gate: No. Full native/replay/size CI is deferred to the adoption PR; this is an
intentionally failing plan/test checkpoint, without a PR or CI watcher.

Production remains held on actual #1962 and a fresh scoped lib-hunk release.
Do not consume its frozen API as a local implementation or weaken these original
assertions at adoption. Recompute the actual base/map before source work.
