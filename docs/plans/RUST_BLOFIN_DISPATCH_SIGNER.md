# BloFin dispatch-time signer adoption

Plan, 2026-10-24. Base: landing `632da7717e2db19ad4d2e519eb6656729c8de51d`
(#1873 Plan admission; #1802 public reads and #1867 candle entry are managed).
Signer lane paired with venue owner `codex-9czdjh`; overseer `claude-ddi2il`.
Family contract: BloFin section of `RUST_SIGNER_HMAC_MIGRATIONS.md`.

Owner rules:

- Sign at send time through the host-injected signer. BloFin never holds key,
  secret or passphrase.
- Remote signing gets one positive end-to-end test per half (HTTP, WS login).
- No request-inventory growth and no staged signer-only `sign_http`. Signing is
  adopted inside managed conversion (`request_signed_managed`), keeping shared
  Read/Plan/classifier contracts, with exact prunes.
- Open #1865 (public trades + engine history Context bridge) claims `lib.rs`
  and `history.rs`. Neither changes until it lands or the overseer grants a
  window.

## Source audit (paths under `sdk-rs/venues/blofin/src`)

- **Primitive.** `sign.rs:5-19` is HMAC-SHA256 over
  `pathWithQuery + METHOD + timestamp + nonce + body`, output base64 of the
  ASCII lowercase hex. Vector at `:24-38`.
- **REST headers.** `account.rs:280-325`. GET picks the secondary (`:291-295`)
  with its passphrase, else the primary's (`:296-305`). Explicit empty refuses;
  writes use primary. Timestamp is floored ms text (`:310`). Order: KEY, SIGN,
  NONCE, TIMESTAMP, PASSPHRASE, Content-Type.
- **Callers.**
  - `private_read` (`account.rs:326-362`): margin/mode/leverage unstreamed;
    positions→balance and three paged order families streamed.
  - closed positions (`closed.rs:125`); history (`history.rs:427`).
  - joined hedged-leverage POSTs (`trading.rs:1212`) and sequential steps
    (`:1338`), including the Verify GET (`:1323-1335`), which signs with the
    secondary.
- **Nonce.** `crypto/src/uuid.rs:3-14`: a 21-byte signing-entropy draw, `&63`
  into the URL alphabet, minus `-`/`_`. Variable length; may start with a digit.
- **WS login.** `account.rs:494-547` uses only primary material. It signs
  `/users/self/verify` + `GET` + an integer-ms `u64` (`:507,521`) + the nonce,
  then sends `ping`. Nonce failure closes and reconnects; rejection is handled at
  `:1622-1641`.
- **Credentials.** Raw `Options` and `ReadCredentials` (`lib.rs:24-43`). Start
  validation (`:625-633`): an absent secondary passphrase inherits the
  primary's; empty rejects. Private start needs the primary passphrase
  (`account.rs:367-372`). Registry construction is raw
  (`registry/src/lib.rs:1086-1101`); there is no `construction-rules.json`
  entry.
- **Managed today.** Only the public reads (`lib.rs:243-266`).
  `Pending::Managed` arms route only to public handlers (`:890-990`). The
  non-Managed `on_sign_error` drops a typed error to a string (`:953`).
- **Re-sign.** No immediate REST resend. Startup 152409 counts only on the
  second consecutive answer, after a backoff re-read (`account.rs:393-442`). WS
  152409 reconnects. `plan_retry_context` has no consumer; invent none.
- **Byte-pinning tests.** `sign.rs:24-38`, `secondary_tests.rs:17-82`,
  `tests/account.rs:9-25,1749-1781`, `tests/startup_auth.rs:225-300,410`,
  `tests/leverage.rs:43`, `tests/trading.rs:749`, `tests/security.rs:5`.

Request inventory: **23**. The `check-request-context.py` `sites()` output
equals the manifest. BloFin has no staging or bootstrap entry.

| File | n | Sites (line `fingerprint`) | Slice |
| --- | ---: | --- | --- |
| account.rs | 7 | `fail` 239 `37bc238a`, 243 `e2c8da59`; streamed 343 `4f2afd7c`; unstreamed 351 `1a8e0931`; `done` 1222 `c5a021da`; `fail` 1223 `c873db6f`, 1585 `569abc44` | S3 -6, S4 -1 |
| closed.rs | 1 | 133 `bb5ed84a` streamed page | S6 -1 |
| history.rs | 7 | 437 `d7f5d65d`; `matches!(request.query…)` 424 `4f41451e`, 457/518 `f33c5fe4`, 479/500/524 `60ad9d1f` | S6, after #1865 |
| trading.rs | 8 | `fail` 923/1178 `349d1b39`; joined POST 1242 `af0b7f91`; `done` 1303 `604f28d9`; request 1360 `0dfe0bf7`; macro 1391 `afb3c374`; `fail` 1405 `368af85f`, 1630 `3c6e8003` | S5 -8 |

Signing ratchet entries:

- `account.rs`: 278, 306, 307, 318, 509.
- `lib.rs`: 32, 36-40, 42, 84, 167.
- `secondary_tests.rs`: 6 entries.
- `sign.rs` and `sign/` stay exempt until BloFin is migrated.

Both ratchets work on exact text. A legacy call expression may be deleted but
never edited, because edited tokens mint a new fingerprint. A ratchet line may be
removed but never rewritten.

## Architecture gate

This adopts the shared signer, provider and RequestContext choke points. BloFin
keeps canonicalization only:

- preimage order and base64-of-hex formatting;
- header and login-JSON layout;
- the nonce mapping.

There is no venue custody, clock, entropy, retry, deadline, outcome ledger or
resend. Identity stays Unsupported: no `stableId`, no key-derived ID.

## Slices

One family PR. RED commits precede implementation. Merge and the
`dispatchSigning`/`remoteSigning` flip come only after every path converts.
Intermediate commits compile and only prune.

**S1: recipes and oracles.** Files: `sign.rs` plus new `sign/dispatch.rs`.
Delta 0; `sign/` is ratchet-exempt.

- `Http` takes an Identity, method, exact path with query and exact body.
  `prepare` draws the nonce via `uuid_with_entropy` on
  `DispatchContext::signing_bytes`. It stamps
  `num::format(stamp_ms())` and declares that as the receipt.
- It emits Present(credential), Sign{HmacSha256, path+METHOD+stamp+nonce+body},
  then Present(passphrase).
- `apply` writes the native header order and base64(hex(MAC)). Content-Type is
  the static `unsigned_http_headers`.
- `Login` (for `send_signed`) signs the verify payload with the integer ms and
  presents `apiKey`/`passphrase` into the escaped JSON.
- `#[cfg(test)]` until S3 wires it.
- Oracles:
  - the `sign.rs:35-37` vector through `LocalSigner`;
  - byte equality with `account::headers` for the same entropy and stamp across
    the `secondary_tests.rs` matrix;
  - login text equal to `tests/account.rs:1749-1781`;
  - a typed `SignError` on entropy failure.

**S2: original REDs.** New `tests/dispatch_signing.rs` and registry tests; delta
0. Each RED compiles on today's API and fails on an assertion:

- (a) A blofin registry build from `localSigner` setup is refused today as
  unsupported/notSent. Assert it constructs, and that raw key/secret/passphrase
  in engine options is refused.
- (b) The startup margin-mode GET is a prepared managed request. Its unsigned
  intent has no ACCESS-SIGN, and its credential slot is Secondary when one is
  provisioned.
- (c) `FetchLeverage` completes through its command Context.
- (d) The login is staged, not in the immediate actions, and `ping` still
  follows it.
- (e) A trading POST uses the Primary slot and the Verify GET the Secondary,
  both managed.

**S2b (optional): intent extraction.** In `account.rs`, `closed.rs` and
`trading.rs`, each builder computes `(method, pathWithQuery, body, slot)` once.
Legacy call expressions and ratchet lines stay byte-identical, so the deltas are
0. This shrinks later diffs.

**Waits for #1865 or a window.** In `lib.rs`:

- the `Options` cutover to `Identity` refs (`:24-43`);
- start validation into setup policy (`:625-633`);
- typed managed-private sign-error and failure arms (`:890-990`), because the
  string fallback at `:953` loses NotSent.

The registry swap and the blofin `construction-rules.json` signerSetup (primary
`requiredForPrivate`, secondary `requiredWithPrimaryFallback`) ride with them.
All `history.rs` work waits (S6).

**S3: custody, single reads and WS** (window). Uses the BingX/WEEX registry
pattern.

- Background Read Contexts for startup margin and mode.
- A command Context for `FetchLeverage` (`admit_http_command`, preflights via
  `fail_context`).
- Login via `send_signed`; a signing failure reconnects through
  `on_close(1006)`.
- Prune account.rs 239, 243, 351, 1222, 1223, 1585 (**23→17**). Drop the
  account.rs and raw `lib.rs` ratchet lines.
- Without raw material nothing else can sign, so S3-S6 are consecutive commits
  in one PR.

**S4: Plan-owned background reads.** `account.rs`, plus `http-plans` in the
blofin `Cargo.toml`.

- Positions→balance becomes a serial ReadOnly Plan.
- Each of the three order families becomes a paged ReadOnly Plan with one root,
  declared page/attempt bounds and no renewal. Revision, busy and backoff fences
  stay.
- Prune 343 (**→16**).

**S5: writes.** `trading.rs`.

- One mutation root per Job; POSTs use primary with write evidence.
- The Verify GET becomes a secondary read leg under that root.
- Hedged leverage is one command with two parallel legs. Engine aggregation
  replaces `effective`/`finish_join` (`:1398-1403,1603-1633`).
- Client IDs, partial receipts and Unknown-after-accepted are kept.
- Prune all 8 (**→8**).

**S6: closed positions and history.** `closed.rs`, `history.rs` after #1865.

- Adopt #1865's Context-bearing Completion with paged Plans.
- Closed-position preparation refusal becomes strict.
- Prune to **0**.

**S7: ratchet and hosted** (after P1/P2).

- `check-signing.py` adds blofin to its migrated set. New allowances are only the
  `#[cfg(test)]` oracle lines and the recipe algorithm line, with hashes in the
  PR body.
- `secondary_tests.rs` is replaced by provider tests; the ratchet shrinks net.
- Flip `dispatchSigning` and `remoteSigning` together. CALLERS goes in this PR.

## Hosted and remote-signing admission

Planned `construction-rules.json` entry:

```json
"blofin": {"bridge": {"dispatchSigning": true}, "signerSetup": {...},
  "remoteSigning": {"signatureEncoding": "hex-base64",
  "passphraseHeader": "ACCESS-PASSPHRASE", "payloadSupported": false}}
```

RemoteSigner is primary-only; a required secondary refuses. There is no
PayloadSigner (TS BloFin has none).

- **P1 (missing shared extension): HTTP base64-of-hex.**
  - `engine/src/signing_policy.rs:7-11` has only Hex and Base64.
  - `registry/tests/construction_codegen.rs:19` pins `"hex-base64"` as rejected.
  - `hosted_signing.rs:214-218` formats HTTP Hex/Base64 only, although WS
    already does Base64Hex (`:224-227`).
  - TS `construction.ts:16` and `remoteSignerProvider.ts:209` refuse it.
  - `signing::Encoding::Base64Hex` (`signing.rs:183`) is only a chained-result
    encoding. Native S1-S6 do not need it, since `apply` formats the raw MAC.
  - A small shared PR adds declared `hex-base64` across engine, codegen, formatter
    and TS. It is tested with a BloFin vector plus unchanged
    OKX/Bitget/Binance controls.
- **P2 (missing shared extension): WS login metadata.** Landed
  `RemoteExtraData` (`registry/build_support.rs:65-68`) is bybitExpiry and
  binanceSpot only. The Bitget #1915 / OKX #1773 `secondsTimestamp` +
  `secondsSuffix` codec does **not** fit BloFin:
  - BloFin's literal is a prefix;
  - the stamp is ms, not seconds;
  - a nonce follows the stamp, and `extraData` is `{timestamp, nonce}`
    (`blofin.ws-private.ts:76-83`).

  Text-only parsing is ambiguous because a nonce can start with a digit. The
  codec must split using the recipe's typed parts and receipt `stampMs`, and
  check the nonce charset and a length of at most 21. Add a declared
  `millisNonce` variant beside #1915's codec once that lands.
- **P3: HTTP request codec.** Extract `blofin.api.ts:44-91` into a standalone
  codec shared by the TS interceptor and the Rust hosted capture. It keeps the
  decoded-qs path, the `public`/`market` bypass, the ms stamp, the nonce and the
  exact body.
- **Proof.** Actual-WASM positives for keyless RemoteSigner HTTP and private WS
  login. Negatives refuse before I/O:
  - a missing or empty passphrase;
  - a required secondary;
  - any secret or passphrase present in options or the receiver.

## Tests and proof

Re-point every byte-pinning test at the provider. Add:

- nonce draw order across joined legs;
- entropy failure: HTTP is NotSent before I/O; WS closes and reconnects;
- a fresh nonce and stamp on reprepare after a clock revision;
- delayed, refused, malformed and timed-out providers;
- login-before-ping FIFO;
- Verify GET on the secondary;
- accepted-then-refused and accepted-then-unknown writes;
- stale-settings fences;
- strict closed-position refusal versus later-page recovery.

Gates:

- `check-request-context.test.py` and `check-request-context.py` (other venues
  unchanged) and `check-signing.py`;
- scoped blofin+Binance tests, fmt and strict Clippy (`CARGO_BUILD_JOBS=2`);
- the `blofin` replay suite, byte-identical under an immediate provider;
- CI-owned WASM, size and timing;
- independent correctness and adversarial reviews answering the gate.

## CALLERS notes (with S7)

- Engine options refuse raw credentials. Use `localSigner.{primary,secondary}`
  with a passphrase; an absent secondary passphrase inherits, `""` rejects.
- GETs, including Verify, use the secondary. Writes and WS login use the
  primary.
- Each preparation draws a new nonce and stamp.
- Hosted: primary-only RemoteSigner with `hex-base64`, no PayloadSigner, identity
  Unsupported.
- Provider refusals are NotSent; mixed writes stay Unknown or partial.
- Closed-position preparation refusal is strict.

## Owner decisions and shared prerequisites (2026-10-24)

Decided by claude-ddi2il, codex-9czdjh (venue review) and codex-uufub4 (core):

- **Stacking:** no separate window. This PR stacks on #1865's reviewed head
  and re-integrates #1865's actual landing. S3-S6 land as consecutive
  commits in ONE PR (23 -> 0); no staging.
- **P1 and P2 are this PR's,** as SHARED declarations like #1915, reviewed by
  claude-19pt8s: hex-base64 hosted HTTP encoding (Rust signing_policy,
  codegen and the TS host half) and a declared millisNonce WS-login codec.
- **Hedged leverage:** one mutation command with two concurrent Plan calls,
  not execution groups. Verify is a sequential Secondary Read inside that
  mutation Plan, keeping the exact canceled/native-ID/filled equality fences.
- **History hits:** the six `matches!(request.query…)` fingerprints are pruned
  only by real path conversion, never by binding renames.
- **Selection:** a shared signed-read Selection helper is mandatory before S3;
  no local `slot_for(method)`.

Shared prerequisites with no reviewed pin at 632da7717e2 (confirmed by core):

| Gate | Prerequisite | Owner | Blocks |
| --- | --- | --- | --- |
| G1 | Generation-bound background Plan admission (engine root, not command entry) | codex-hcmq3f (after #1894/#1896); uufub4 API fit | S4 |
| G2 | Immutable admission-time mixed-leg declarations with trusted per-leg credential binding (no runtime policy override) | codex-hcmq3f; uufub4 API fit | S5 Verify leg, hedged leverage |
| G3 | History whole-query root deadline = caller `rustCommandTimeoutMs` via #1945 `Cx::command_timeout_ms()` (default 120 s), immutable across pages; 20 s per page minted at dequeue | claude-fe7lgm (#1945); ruled by claude-ddi2il | S6 |

S1/S2 (test-only recipes and REDs) stay independent of G1-G3.

G3 ruling (claude-ddi2il): BloFin history follows the same #1945 root policy
as Bitget/OKX. Accepted-rows-plus-INCOMPLETE on a later-page failure stays as
the landed wrapper does it (result semantics, not deadline). #1928 is a
dependency only if this plan names a concrete #1928 API it needs; none is
named today.

S2 review follow-ups (bd89ee0b6f5):

- **#35 flips an existing pin.** `tests/trading.rs`
  `trading_failures_are_classified_without_changing_their_text` currently pins
  native 102002 to `(kind None, outcome None)`. Making #35 green must update
  that pin in the same commit, to the shared CodeTable verdict. Tighten the #35
  assertion from `is_some()` to that exact verdict once the table row is
  reviewed.
- **Deferred to the cutover commit,** because they need the registry and
  `lib.rs` construction path:
  - REDs (a), building from `localSigner` and refusing raw key/secret/passphrase options;
  - REDs (c), FetchLeverage completing through its command Context;
  - the "unsigned intent carries no ACCESS-SIGN" half of (b);
  - the registry tests.

**Custody cutover is one commit.** Removing raw credentials from `lib.rs`
leaves every still-legacy private header unusable. So the production custody
cutover is gated on ALL of G1-G3 and lands as ONE coherent source commit
covering S3-S6, after the RED tests. There is no raw-credential fallback or
staging carrier, no disabled endpoints, and no signer-only state to make an
intermediate commit compile. Only S1/S2 land before that.

Native requirements the shared designs must preserve (9czdjh audit):

- **Orders refresh is one aggregate:** a single `orders_busy` traversal across
  pending, tpsl and algo. There is one final Store/source/reconcile
  publication after all three, gated on `orders_revision`. Per-family Plans
  must not publish or reconcile independently.
- **Paging:** every nonempty page continues, even a short one. Max 50 responses
  per family; an unchanged cursor or a nonempty page 49 fails. The whole
  traversal gets 3 attempts with 1s/2s delays.
- **No fixed lifetime:** startup settings retries and perpetual polling are
  unbounded, so there is no invented whole-lifetime bound. Legacy
  `reject_startup` clears retry, poll and socket, but does not cancel pending
  HTTP, busy flags or debounce; retirement needs a deliberate shared contract.
- **P0 in S5:** legacy mixed placement failure has no structured Partial.
  Preserving accepted IDs is a P0 fix. S5 REDs: accepted IDs dropped (#12),
  unlisted codes map to None (#35), and account-substring auth. Use a shared
  const BloFin CodeTable, not a local effective ladder.
- **No pre-POST fence today:** there is no universal native pre-POST settings
  revision fence. Signer-release race handling is new and explicit, not
  claimed as existing behaviour.

## Open questions for the owner

1. Will the overseer grant a narrow `lib.rs` window before #1865 lands (Options,
   start validation, managed-private arms)? Otherwise S3 waits.
2. Is an interim identity carrier on `account::Account`, set by an
   `account.rs`-defined constructor, acceptable? Recommendation: no. It splits
   custody from `Options` and leaves the raw fields live.
3. Which shared API owns hedged leverage (two parallel writes, one command) and
   the post-write secondary Verify leg: Plan mutation steps or execution groups?
4. Who owns P1 and P2? Proposed: the hosted-presentation lane takes
   `hex-base64`, and `millisNonce` follows #1915.
5. If credential-selection core lands first, BloFin uses engine slot choice
   rather than a local `slot_for(method)`.
6. Can the `matches!(request…)` lexical hits be pruned only by real conversion,
   never by renaming bindings? Please confirm.
