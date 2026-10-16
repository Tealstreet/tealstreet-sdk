# PR 7408 findings and measured evidence

Initial gates and query/load mutations: `4ea43b4de13f28647153033c2d1620cfc213eb33`.
Post-refresh checks/tests: `a2355f28fd9017d5f557551c88524fcb4d0d5e7b`.
First clock-contract repair: `912d5e5e5f847733d40fc023662edf97f50fbe9a`.
Scoped clock-contract gates: `4e8a65338c6839630cd2ff9ae8f16b57fe454e83`.
This evidence file is a later documentation commit; its SHA does not replace the
code SHA attached to each command in PR_7408_GATE_EXITS.json. The initial code commit has parents
`87ee289cc8` (our fixes) and requested integration `4a7b848e03`.
Earlier integration merges were `8fd9e9443b` and `c030934010`. The refresh commit
has parents `d7b151b1f6` and integration `f436fc8163`; its exemption conflict kept both host lists.
Refresh inputs changed (transport fixtures and exchange timer/runtime code), so affected
scoped checks and tests are remeasured rather than relabeled. Native wire/query implementation
and boxed loader files remained byte-unchanged; the signing parser test now places its legacy
native-key assertion before its wire assertion.
The inherited query ancestors `fe9530f6f2`, `a821d586a0`, `e4a29eab4a`,
`7fcaebd598`, and `c5325263cc` remain ancestors. The scoped sub-agent review
covers the changes described here, not an exhaustive certification of those ancestors.

Final refresh code under measurement: `fa686d4f83db70848568771cb7f9ddb6c39ca541`,
with parents `d2932ad716` and latest fetched integration `1181108d2d`.
The fixture conflicts retained the explicit clocks; exemption conflicts retained
both lists. Scoped all-target checks and changed factory/WS witnesses are remeasured.

## Decisions and source boundaries

The additive route restores `RestRequest.params: Option<Value>` and the legacy
Value serializer API. Native `query_params` uses an explicit `QueryProfile`.
The separate native signer DTOs decode retained tokens directly into QueryValue;
no From implementation, Value conversion shim, or query JSON projection participates
in that decoder or URL/signing preimage rendering. Existing RemoteSigner hosts
keep their exhaustive legacy HTTP/WS API. The new `sign_query` capability is
opt-in: its default refuses unsupported native requests. HOW/diagnostic JSON
projection remains an explicit physical protocol boundary.

BaseExchange and legacy RemoteSigningAdapter again invoke `try_parse_json`, which
calls `normalize_json_numbers` and recursive `normalize_object_order`. Native
`try_parse_query_json` uses `JsObjectMap` at every object level. At original PR
head `04cbdff789`, QueryValue already used JsObjectMap and from_wire_capture
already collected directly into it. JsObjectMap::FromIterator -> extend -> insert
sorts canonical array-index keys after every insertion, with no DerefMut bypass.
Thus the alleged textual-order signing regression in finding 2 was not present
at that head. The old HOW test hand-construction did obscure which operation
was being tested; it is restored to parsing textual Value input, while a separate
test asserts decoded native signing keys/values before URL or wire assertions.

## Counts

Python 3.11 tomllib, iteration 1 over the fixed eleven assigned manifests:
127 total = 94 pending, 22 ported, 10 divergent, 1 not-applicable.
Against both requested integration `4a7b848e03` and actual refreshed parent
`f436fc8163` and latest parent `1181108d2d`, exactly 25 rows moved off pending:
15 pending->ported and 10 pending->divergent. The original 17/8 split is superseded
by the declared CHOSEN open-string order context boundary (two rows).
The remote adapter's pre-existing ported row also becomes MIXED divergent for
its explicit opt-in native host protocol; this is not another row off pending.
All 94 executable venue loader entries remain pending on this PR. Their separate
WIP branch is `feat/rust-core-registry-loaders`, commit `10b32f5d24`; it contributes
zero moved rows to this delivery.

## Additional integration timestamp repair

At `4e8a65338c`, Breakoutprop check exited 101 at E0063 because integration added
Ticker.timestamp and the quote initializer omitted it. The source mapper at
breakoutprop.exchange.ts:330-345 does not emit timestamp. Commit `dfcece8bc4`
sets None; `d2932ad716` asserts that decoded native field on the two independent
stock ticks for each of the two ticker source methods, before JSON projection.
Mutation Some(0.0) fails at QUOTES native absent timestamp (101), restoration
passes (0), at baseline d2932ad716. The old projection helper did not observe
this field; the new native assertions close that specific blind spot.

## Controls and survivors

Deleting only `normalize_object_order(&mut params)` in
HOWWebSocket::request_http survives (exit 0): HOWWebSocket::send still invokes
js_json_stringify, whose js_own_keys independently orders integer properties.
This survivor is retained in query-order-control.py and is a finding about the
redundant call site, not a discarded control. The complete frame-order control
removes HOW ordering and bypasses that final ordering serializer. The restored
textual-input tunnel test must fail at `HOW integer query property order`, then
pass after restoration. This tests actual integer-order behavior rather than
requiring a redundant line to be essential. Its fixture contains source-style
textual top-level and nested integer keys and both HOW raw modes.

Mutation enumeration is a floor: iteration 1 of the explicitly listed controls;
no exhaustive mutation, numeric-domain, adapter-stack, or venue census is claimed.
New native assertions precede wire assertions and expected keys, depths, call
counts and source corpus dimensions are independent of the returned artifacts.

## Integration clock repair

The first refreshed Kucoin factory test exited 101 at E0046: integration made
ExchangeRuntime::monotonic_now_ms required and that fixture did not implement it.
This failed command remains in the exact-SHA ledger at a2355f28fd; it is not
reported as a pass. Commit `912d5e5e5f` supplies explicit fixture clocks in 20 files
across the scoped venues. Existing WS monotonic fields are reused; static mocks
return their fixed clock and the API retry clock advances only nonnegatively.
The next Bybit check exited 101 at E0407: four fixtures retained the removed
WsRuntime::monotonic_ms method. Commit `4e8a65338c` moves those complete method
bodies into ExchangeRuntime::monotonic_now_ms and removes the old members,
retaining Kucoin performance and OKX clock trace effects. Both failures remain
in the ledger. The production trait remains required, with no wall-clock fallback default.
Sub-agent review enumerated 22 direct ExchangeRuntime implementations in these
seven crates (iteration 1); all now implement the method, including two pre-existing
MEXC advancing clocks. This is a scoped source census, not a workspace host census.

## Unpassed additional status gate

The extra `cargo run -p safe-cex-parity --bin parity-check` exits 1 at baseline
`d2932ad716`: safe-cex-webworker/src/twap_manager.rs:78 uses thread_local!, which
this indexer refuses without generated declarations. That source file is byte
unchanged from integration f436fc8163 (git diff exit 0); the problem is outside
this query repair. The gate was not weakened or bypassed. Full Rust declaration
resolution remains unproved; extractor success is not that claim. In particular,
this run does not certify every newly added mapping against the public-item index. The required
extractor check remains a separately measured gate. This unresolved status finding
is disclosed even though it is not among the nine original review findings.

## Review and limits

A native sub-agent found three defects during this repair: native signer auth
fallback/empty URL handling differed from the legacy path; RateLimitedAdapter
failed to forward the URL-query capability; and the owned depth comparison used
>128 instead of canonical >=128. All three are corrected. The native signer unit
asserts usable auth restoration and unusable/case-duplicate removal, and the
native decode test pins both depth127 acceptance and depth128 refusal.
The Kucoin factory test proves production profile inheritance and exact URL
rendering. It does not prove authenticated native Kucoin HMAC parity against
TypeScript. Existing legacy factory source tests are a separate claim.

No workspace build, host clippy, wasm clippy/build, full monorepo suite, CI rerun,
or live network/credential gate is claimed.

The latest merge introduced a duplicate Polymarket fixture method without a
textual conflict. Its check at fa686d4f83 exited 101/E0201. Commit `9c4a3a0be6`
keeps integration's advancing monotonic clock and removes our duplicate fixed
method in both fixtures. This changes no transport/exchange/registry source or
test input (git diff exit 0), and its own crate is rechecked at `9c4a3a0be6`.

## Per-finding verdicts

1. FIXED — additive legacy Value ABI restored; transport/exchange all-target checks and seven named caller crates, plus two no-Serialize/no-From compile-fail doctests.
2. REBUTTED — original PR already collected into recursively ordered JsObjectMap. Direct decoded-key assertions now cover both signing parsers. Legacy normalization and textual-input tunnel fixture restored; complete ordering removal fails the named assertion (101), restoration passes (0). The redundant-call survivor is disclosed above.
3. FIXED — production Kucoin factory selects the native profile; native factory inheritance/exact-rendering test passes. This is additive opt-in, not an all-venue cutover or authenticated native HMAC proof.
4. FIXED — exact-SHA command exits are recorded in PR_7408_GATE_EXITS.json; scoped query tests and mutation controls are measured, not inferred from a dependency library build.
5. FIXED — query_in_url capability preserves HOW params and forwards through serializer, HTTP, remote-signing and rate-limit adapters; query units assert native retention and legacy falsy retention.
6. FIXED — two open-string context rows are CHOSEN divergent with reasons, confirmed by independent tomllib census. No order behavior was rewritten.
7. FIXED — boxed callable/future load shape supports two heterogeneous closures in one map; native load test and eager-evaluation control discriminate (101->0).
8. REBUTTED — correctness defect not established: reviewer independently resolved all 47 stock metadata triples correctly. The original 7,231-case live corpus proof is bound to 38ccee6f47 in CORE_P4A_GATE_EVIDENCE.md; exact order implementation/test bytes remain unchanged (git diff exit 0). The copy remains an explicitly CHOSEN frozen projection pending consolidation with canonical metadata ownership, which integration 89ad07ac80 has now supplied. This repair does not claim that consolidation.
9. FIXED — native SignedConfig omits absent data symmetrically; retained native decoding plus direct JS numeric-token serialization preserves exponent bytes; owned depth127/128 boundary and decoded numeric/string identities precede wire assertions in query_params.

Each verdict's exits are associated with the measured SHA(s) in the JSON ledger,
not attributed to the later evidence commit. The command summary below records
actual scope and ignored tests; there is no workspace compilation claim.

## Measured command scope

All commands ran serially for Cargo, with jobs 2 (the inherited query mutation
runner uses jobs 1). Native source processes used
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
The full exact command/exit/SHA history, including the corrected compile
failures and unpassed status checker, is committed in PR_7408_GATE_EXITS.json.
No running command is counted as a pass.

The selected transport targets passed 117 tests at `912d5e5e5f` (their entire
src/tests tree is byte-unchanged through fa686d4f83; git diff exit 0): P2C 35,
query_params 6, tunnel 69, try_parse_json_numbers 4, parser_overflow 1 and
parser_signed_zero 2. Exchange query units passed 4; lifecycle 14 passed/1 ignored;
health/HTTP/token 19 passed/1 ignored. Ignored tests were not executed.
Two query_value compile-fail doctests passed at `4ea43b4de1`. Five query mutations
compiled to named 101 failures and restored to 0 at `4ea43b4de1`. The native loader
control compiled to 101 and restored to 0 at that SHA; two native_load tests passed.
The ordering control was repeated at `912d5e5e5f` (survivor 0; complete removal 101;
restoration 0). The native timestamp control compiled to 101 and restored to 0 at
d2932ad716; its restored existing source test observes 45 pinned cases, including
four native timestamp omissions across the two stock rows/two ticker methods.
Enumeration stops at these fixed targets/corpora and one iteration of each
listed mutation; no exhaustive source or mutation census is implied.

## Final measured summary

| Scope | Measured code SHA | Exit / result |
| --- | --- | --- |
| transport, exchange, registry all-target checks | fa686d4f83 | 0 each |
| Bybit, WOO, Kucoin, MEXC, OKX, Breakoutprop all-target checks | fa686d4f83 | 0 each |
| Polymarket all-target check | 9c4a3a0be6 | 0 |
| Exchange query units/lifecycle/health-HTTP-token | 9c4a3a0be6 | 0 each; 4 / 14+1 ignored / 19+1 ignored |
| Kucoin production native factory / existing four-ping clock witness | 9c4a3a0be6 | 0 each; 1 test each |
| OKX existing private/business native clock witness | 9c4a3a0be6 | 0; 1 test |
| Extractor writer and check (output inspected) | fa686d4f83 | 0 each; 7341 symbols/512 files; zero updates/additions/orphans |
| Extra full Rust parity status checker | d2932ad716 | 1; integration thread_local indexing refusal; not passed |

Transport/exchange/registry source and test trees are byte-unchanged between
fa686d4f83 and 9c4a3a0be6 (git diff exit 0). The successful transport target suite
and controls retain their original SHA bindings; they are not relabeled as runs
at the final documentation commit. The final resolved branch still has the
original query ancestors listed above; the sub-agent review is scoped to these
fixes, not a blanket certification of those ancestors.

## Final e9b4202fc6 integration refresh

Requested fetch/merge-down was repeated after the evidence commit. Integration
advanced to e9b4202fc6; merge `346e1c8803` has parents deea2ba83a and e9b4202fc6.
Its WOO fixture conflict keeps the explicit fixed clock; exemption lists retain
both additions. Incoming websocket implementation changes invalidated the
exchange byte-comparison, so the scoped checks and witnesses were rerun.
At measured SHA `346e1c880364c31922af9b1c2637cfa4e44eb183`, all ten named all-target
checks exit 0, the three factory/WS tests exit 0, and exchange units/lifecycle/
health-HTTP-token exit 0 (4 / 14+1 ignored / 19+1 ignored). Extractor writer/check
both exit 0 with inspected zero-update/addition/orphan output. Transport's entire
src/tests tree remains byte-unchanged from 912d5e5e5f (git diff exit 0), retaining
the 117-test proof at its actual SHA. No successful status-checker claim is added.

## 89ad07ac80 integration refresh

Final fetch advanced integration to 89ad07ac80; merge 8f63c5f3c0 has parents
6c6fa442ac and 89ad07ac80. Only exchange source/tests changed among the primary
three crates; the seven scoped venue source/tests trees are also byte-unchanged.
At measured code SHA 8f63c5f3c055739e2daa59cc9cb0d1701f53e812, transport/exchange/
registry all-target checks exit 0, exchange query units/lifecycle/health-HTTP-token
exit 0 (4 / 14+1 ignored / 19+1 ignored), and inspected extractor writer/check exit 0
with 7341 symbols/512 files and zero changes/additions/orphans. Earlier venue
gates retain their 346e1c8803 bindings; no whole-workspace gate is claimed.

## Final preservation before push

Final fetched integration: `9c151409d125381f67d08d2b644afa89c77a79c8`. Merge `649816d5aa3303728e01b370612f30e77a7b746d` has parents `a516c1a0b3446190545ebeae63db8218cf5f61b3 9c151409d125381f67d08d2b644afa89c77a79c8`.
Primary transport/exchange/registry src/tests compare byte-identical to measured
code `8f63c5f3c0` (git diff exit 0). This is the unchanged-input case of the
required pre-push preservation check; no new Cargo exits are attributed to this
merge or the following evidence-only commit. The required extractor gate remains
bound to its actual measured code SHA. No CI rerun or forced push occurred.
