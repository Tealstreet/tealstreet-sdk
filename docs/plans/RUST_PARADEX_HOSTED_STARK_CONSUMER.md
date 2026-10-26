# Paradex hosted Stark consumer (mode 2)

Status: PLAN ONLY. No source, Cargo feature, export, host change, activation or
deployment is authorized by this document. #2126 (live scalar exclusion) stays
HELD until the working route below exists, including today's batch path.
Owner: claude-m26ps4. Reviewers: codex-9czdjh (shared envelope, Paradex #8169
decisions D-a/D-b/D-c), claude-kuhwxi (security, required). Landing follows the
exact-tree GO from claude-8x5tm1.

Basis: [RUST_HOSTED_SIGNING_ENVELOPE.md](RUST_HOSTED_SIGNING_ENVELOPE.md) (#2156,
"Second declared consumer: Paradex Stark", amended here for batch), engine
sign-meta (#2163), tealstreet-next PARADEX_HOSTED_STARK_AND_SUBKEYS.md (#8169)
and its device originals ea795490356. Mode 3 (server-held subkeys) is out of
scope and still waits for Sam's explicit infrastructure go.

## Why

Mode 2 replaces today's live uploaded-key signing for hosted Paradex accounts:
the device keeps the Stark scalar and signs, and the server never receives it.
It must therefore keep every operation the live path performs today, including
native batches of 1..=10 plain LIMIT orders (overseer ruling). It must not
reintroduce the legacy digest-only `payload_request(hash)` route: the device
signs only what it can rebuild from a closed descriptor.

## Order of work

1. SDK: the closed descriptor, the one native projection and digest builder,
   peer validation and a pure WASM export, behind a non-default feature.
2. SDK: the Paradex origin moves its delegated path from `payload_request(hash)`
   to managed `Action::Sign` with StarkDigest operations and the descriptor
   (sign-meta gives owner/slot/role). The legacy route and Rust's delegated
   `orders.len() > 1` refusal (trading.rs, "payloadSigner ... signs one order
   per request") are removed in the same cutover. That deliberately changes
   capability: in Rust the delegated route IS the mode-2 device route, and no
   Rust payloadSigner path remains to keep the old limit. The batch capability
   exists only when the peer negotiates `paradex.orderBatch/1`. The legacy
   TypeScript `payloadSigner` in tealstreet-next safe-cex is untouched and keeps
   its one-order limit. No dual route.
   Paradex is the only user of the SDK's bare-digest `PayloadSignRequest` route
   (engine action, WASM host and native transport handlers). Step 2 removes it
   entirely, as an orphan, in the same cutover (kuhwxi M3). A ban then forbids
   reintroducing `PayloadSignRequest` or any digest-only "stark"/"felt-pair"
   payload route.
3. tealstreet-next device consumer, after Copybara imports step 1 and the shared
   `@tealstreet/safe-cex-wasm/hosted-signing` wire module (codex-9czdjh) lands.
   `buildInitPayload` key exclusion and the public-subkey descriptor ship in this
   step together with the working route, never earlier and never as a
   `publicOnly` flip.

No NX-local typed shim, no TS Stark math and no second wire spelling.

## Frame

Exactly #2156: `sign-req.d = {id, type:'op', v:2, exchangeName:'paradex',
accountName, request:{attemptId, deadlineMs, receipt, slot, role, binding,
descriptor}}`; `sign-res.d = {id, type:'op', v:2, attemptId, result}` or the
closed error arm. Never legacy `config`/`wsAuth`. `attemptId` is a JSON number
that must be a nonnegative safe integer; the receipt nonce stays a canonical
decimal string.

## Descriptor

One family-tagged closed object at `request.descriptor`. Every object is closed
(unknown keys refuse). Keys are the literal spellings below; each Rust variant
struct carries its own `#[serde(rename_all = "camelCase", deny_unknown_fields)]`,
so nothing relies on enum-level renaming. Integers are JSON numbers that must be
nonnegative safe integers (at most 2^53-1), refused rather than rounded.
Decimals are the canonical native `num::format` strings. Each optional key is
REQUIRED-PRESENT, with a value or `null`; null and absence are not
interchangeable.

```text
{ "family":"paradex", "op":"auth", "revision":"paradex/1",
  "timestampS": int, "expirationS": int }
{ "family":"paradex", "op":"orderCreate", "revision":"paradex/1",
  "target": "orders"|"ordersBatchSingle", "order": Order }
{ "family":"paradex", "op":"orderBatch", "revision":"paradex/1",
  "items": [Order, ...] }                                   // 1..=10
{ "family":"paradex", "op":"orderModify", "revision":"paradex/1",
  "id": string, "timestampMs": int, "market": string, "side": Side,
  "orderType": ModifyOrderType, "size": decimal, "price": decimal,
  "triggerPrice": decimal|null }

Order = { "timestampMs": int, "market": string, "side": Side,
  "orderType": OrderType, "size": decimal, "price": decimal|null,
  "triggerPrice": decimal|null, "instruction": Instruction,
  "flags": ["REDUCE_ONLY"]|null, "stp": "EXPIRE_TAKER"|null,
  "clientId": string|null }
Side = "BUY"|"SELL"
OrderType = "LIMIT"|"MARKET"|"STOP_LIMIT"|"STOP_MARKET"|"TAKE_PROFIT_MARKET"
ModifyOrderType = "LIMIT"|"STOP_LIMIT"|"STOP_MARKET"|"STOP_LOSS_LIMIT"
  |"STOP_LOSS_MARKET"|"TAKE_PROFIT_LIMIT"|"TAKE_PROFIT_MARKET"
Instruction = "GTC"|"IOC"|"POST_ONLY"
```

The create and modify type sets are separate and closed. Modify mirrors native
amend admission: a reconciled LIMIT that is NEW/OPEN, or an UNTRIGGERED order
of a native `is_trigger` kind, using the venue's own type spelling. This does
not widen create.

Negotiated recipe ids map one to one and closed: `paradex.auth/1`,
`paradex.orderCreate/1`, `paradex.orderBatch/1`, `paradex.orderModify/1`. Batch
is advertised separately. A peer without `paradex.orderBatch/1` makes the origin
refuse a batch Unsupported/NotSent before queueing; it is never split into
single orders.

## Presence table (derived from native `prepare`, `native_batch` and modify)

Native trading.rs is the authority. These rows transcribe its current behavior.
They are pinned by originals that compare against native fixtures, not newly
asserted rules.

| orderType | price | triggerPrice | flags | Native source |
| --- | --- | --- | --- | --- |
| LIMIT | value | caller's trigger_price, value or null | REDUCE_ONLY iff reduce_only, else null | kind LIMIT (including a MARKET with an execution price, which becomes IOC LIMIT) |
| MARKET | null | caller's trigger_price, value or null | REDUCE_ONLY iff reduce_only, else null | kind MARKET |
| STOP_LIMIT | value | value (required) | null (reduce-only refused natively) | Conditional |
| STOP_MARKET | null | value (the order's price) | ["REDUCE_ONLY"] | StopLoss |
| TAKE_PROFIT_MARKET | null | value (the order's price) | ["REDUCE_ONLY"] | TakeProfit |

- instruction: "IOC" when an execution price converted the order; otherwise
  IOC for IOC/FOK, POST_ONLY for PostOnly, and GTC otherwise.
- stp and clientId: both non-null for a top-level order, both null for an
  attached child. clientId is 1..=64 chars with no CR/LF, unique within a
  command (native `validate_caller_ids`).
- Batch item, and orderCreate with target `ordersBatchSingle`: the same
  batch-eligible projection. That is orderType LIMIT, triggerPrice null, a
  top-level order (stp and clientId non-null, unique across items),
  flags `["REDUCE_ONLY"]` or null, and instruction GTC|IOC|POST_ONLY. Only
  target `orders` admits the other order types.
- Provenance is checked by the origin only. native_batch also excludes an
  original FOK time-in-force, close and attached legs. Those facts are absent
  from the wire projection (FOK and IOC both emit "IOC"), so native ORIGIN
  admission enforces them. The peer can't infer them and the plan doesn't claim
  it does.
- Modify: id is non-empty and renders the one encoded path component; price is
  always present; triggerPrice is non-null exactly for the `is_trigger` kinds,
  per the native modify builder.

## What is hashed and what is authorized

Hash only what native hashes, through the one native builder:
- auth: POST, `/v1/auth`, body 0, timestampS, expirationS;
- order: timestampMs, market, side, orderType, quantum(size), and quantum(price) or 0;
- modify: the same plus id.

The unhashed fields (triggerPrice, instruction, flags, stp, clientId and the
create target) are still part of the authorized operation.
- Origin: ONE projection function maps the frozen native Placement/Modify to the
  descriptor. Before sending the frame, the origin asserts that the descriptor
  equals the projection of the exact body and target it will publish,
  including the encoded modify id. This is an honest origin's self-check.
- Peer: validates every closed value and the presence table above, and binds
  account, chain and expected public subkey from its OWN installed slot. Only
  then does it hash. It signs nothing it can't fully describe.

Residual risk, stated plainly (kuhwxi M1): Paradex's signature does not cover
the unhashed fields. That is the venue's signing scheme, exactly as with local
signing today. A compromised origin could publish different unhashed fields
beside a valid signature, or pair the signature with another body within the
venue's validity window. The device's guarantees are key custody, correctness
of the hashed fields, policy conformance of the described operation, and the
time bounds. Binding the unhashed fields rests on origin integrity.

## Pure builder and export

```rust
pub fn digests(d: &ParadexDescriptor, b: &ParadexBinding) -> Result<Digests, DescriptorError>;
pub struct Digests(/* 1..=10 */ Vec<[u8; 32]>);  // auth/create/modify: exactly 1; batch: N, item order
pub fn validate(d: &ParadexDescriptor, b: &ParadexBinding, peer_now_ms: f64, deadline_ms: f64)
    -> Result<(), DescriptorError>;
```

Both times are raw milliseconds. Before anything else, validate refuses a
non-finite (NaN, ±Infinity) or negative peer_now_ms or deadline_ms with a closed
code, so a NaN can never pass "equal or later refuses". The deadline comparison
uses the raw values, fractional ones included (#2156 keeps fractional bounds).
Only the native skew checks convert units, flooring exactly as native does
(`now_ms as u64`, seconds = ms / 1000). The declared ≤5 s hosted-capture cap
is unchanged.

Local signing (`Credentials::auth/order/modify`) is refactored to call the same
`digests` before its local Stark sign. Local and hosted therefore hash the same
bytes, and existing sign vectors stay byte-exact. `ParadexBinding {account,
chain_id}` is built only from trusted config/setup. `DescriptorError` is closed
and sanitized.

Pure WASM export, linked only into the Paradex/device host build:

```text
paradexHostedDigests(descriptorJson: string, bindingJson: string,
                     peerNowMs: number, deadlineMs: number)
  -> { "digests": ["0x<64 hex>", ...] }   // length 1, or N for orderBatch, in item order
   | { "error": "<closed code>" }
```

The export reads no clock and holds no key. The device passes its own clock read
at the actual signing moment. After any async work it calls again, rechecking
the deadline and clock revision immediately before signing; validation is never
cached from receipt time.

## Signing and result

One request, one receipt, one task, with the shared #2156 operation graph. The
peer builds ONE SignRequest on its own installed same-slot refs:
`Present(account credential)` followed by N StarkDigest `Sign` operations
(N=1 except batch, N≤10, so at most 11 operations, inside the 32 limit). The
result is exactly N+1 values in that order: first the exact credential bytes,
then N 64-byte signatures in item order. Each r/s is range-checked and verified
against its own recomputed digest and the same bound public subkey.

The result is all-or-nothing: if the deadline passes, a cancel arrives or a
signature fails during partial signing, there is no partial result. The origin
checks count, width and order. It compares the Present bytes with the exact
expected credential (D-a) and verifies each signature against its digest and
the bound subkey. Only then does it render the native credential header and
decimal felt pairs into the native body. The startup PublicKey(StarkDigest)
check of the public subkey is separate and unchanged.

## Time policy

These are the #2156 security pins, applied at the actual signing moment:
- the original deadline is checked first, and equal or later refuses;
- auth requires expirationS == timestampS + 300 exactly, with timestampS within
  peer_now ±30 s;
- order, batch-item and modify timestampMs must be within peer_now ±30,000 ms,
  inclusive.

## Originals before source (SDK)

Positives:
- auth;
- orderCreate target `orders`, for each create orderType;
- orderCreate target `ordersBatchSingle`, for a plain LIMIT only, with and
  without reduce-only;
- a reduce-only LIMIT inside a batch;
- batches of 1 and 10 items;
- modify for LIMIT and for each native `is_trigger` kind (UNTRIGGERED).

Each positive is checked against native fixtures with WHOLE-operation native
body equality: the local path and the hosted path produce byte-equal bodies,
signatures included. Digest equality alone is not enough. An independent Stark
verifier checks each signature.

Refusals, all with zero callbacks and no HTTP:
- descriptor shape:
  - every unknown or missing key, and every null/absent swap;
  - an unsafe, negative, fractional or string integer;
  - an exponent or `+` decimal;
  - a presence-table violation;
- batch:
  - a 0- or 11-item batch;
  - duplicate client ids;
  - a bad middle item;
  - a non-eligible item;
  - an un-negotiated batch;
- operations and targets:
  - a non-LIMIT order or a triggerPrice on target `ordersBatchSingle`;
  - a create-only type (MARKET) on modify, and a modify-only type on create;
  - a Present credential that differs from the expected credential;
  - each excluded op (cancel-by-signature, onboarding, transfer, withdraw, unknown);
  - a method, path or subkey substitution;
  - a descriptor carrying a digest;
  - an origin projection mismatch;
- signatures:
  - swapped, missing, extra or invalid signatures;
- timing:
  - a NaN, ±Infinity or negative peerNowMs or deadlineMs, refused before any
    digest (a fractional finite value is valid);
  - each time-policy edge (inclusive bounds);
  - a deadline or cancel during partial signing, with no partial result.

## Gates

The feature is off by default. Non-adopters show zero byte growth (paired
Bybit, Bitget and Kraken). One local composed combined measurement runs on the
current tip, plus Paradex when it is adopted. Scoped tests, strict checks, bans
and ratchets apply, followed by one frozen-head CI run, READY and an exact-tree
GO. The NX device consumer has its own PR to tealstreet-next master, which the
overseer merges after GO, with no deployment.
