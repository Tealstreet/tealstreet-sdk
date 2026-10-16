# HOW remainder: measured gates and independent controls

Branch `feat/rust-p2b-how-rest`, integration base `bfe57deb48`.
The HOW manifest slice is **100 rows: 80 ported, 20 divergent, zero pending**.
All 49 formerly pending rows have a disposition. The global index has 6,901 rows
and 433 public Rust items. The relay constructor is divergent for the inherited
header ABI and omitted arbitrary Node options; its typed auth surface is mapped.

Behavioral source: [HOW guidance](../../../packages/http-over-websocket/CLAUDE.md).
Host contracts and representation limits: [transport README](README.md).

## Required gates

Commands below run with the real Yarn 1.22.21 binary and rustup bin directories
on PATH. Cargo 1.99 requires `-- --include-ignored`; a bare Cargo flag is invalid.
Tests are package-scoped, including the live routing and diagnostic tests.

| Gate | Positive command and outcome | Single negative control and outcome |
| --- | --- | --- |
| Format | `cargo fmt --all --check`: 0 | Append an unformatted private function: 1 |
| Strict clippy | `cargo clippy -p safe-cex-transport --all-targets`: 0 | Insert one unused local in `TunnelDialOptions::headers`: 101, unused-variable diagnostic |
| Tests, ignored included | `cargo test -p safe-cex-transport --all-targets -- --include-ignored`: 0; 104 passed, 0 failed, 0 ignored | Disable TSB1 magic validation, same full command: 101, corpus assertion fails |
| Wasm build | `cargo build -p safe-cex-transport --target wasm32-unknown-unknown`: 0 | Add wasm-only `compile_error!`: 101 |
| Extraction | `yarn parity:extract --check`: 0, no changes/additions/orphans | Rename one inventoried TS constructor symbol to a nonexistent name: 1 |
| Status | `yarn parity:check`: 0 | Clear the layer on a mapped symbol, keeping its mapping: 1 (status-only failure) |
| Invention | Same check: 0 | Add an unmapped public Rust function: 1 |
| Report | `yarn parity:report`: 0 | Same unmapped public function: 1; report refuses invalid inventory |

Plain clippy is strict because workspace warnings are denied; this does not rely
on an extra CI `-D warnings` flag. The unmodified positive commands ran after
restoring mutations. No whole-monorepo or unrelated SDK package suite was run.

Reproduce these negatives, from repository root, one runner at a time:

```sh
python3 sdk-rs/safe-cex-transport/scripts/how-gate-controls.py
python3 sdk-rs/safe-cex-transport/scripts/how-invariant-controls.py
python3 sdk-rs/safe-cex-transport/scripts/how-behavior-controls.py
python3 sdk-rs/safe-cex-transport/scripts/how-relay-controls.py
```

The runners restore each changed file in `finally`, save full `/tmp/p2b-*` logs
and result JSON, and reject a compile-only failure for behavioral assertions.
Run them without concurrent builds or parity commands in this worktree.

## Binary-envelope independence

`binary_envelope_guards_report_the_specific_invalid_component` includes a valid
TSB1 header/payload control and a valid maximum-size JSON header, padded only
with JSON whitespace. Each malformed entry is valid through the preceding
stages; an exact error reason identifies the stage it reaches.

| Stage | Corpus shape | Independent mutation | Result |
| --- | --- | --- | --- |
| Prefix | Four bytes and seven bytes, separately | Disable `< 8` | 101; a panic cannot stand in for the expected decode error |
| Magic | Wrong magic, valid length, valid HTTP header, nonempty payload | Disable magic comparison | 101; otherwise accepted |
| Zero length | Valid magic; declared zero header length; remaining bytes present | Remove only `length == 0` | 101; expected length error changes to response-header error |
| Maximum length | Valid magic; valid JSON plus whitespace of size 1 MiB + 1; payload present | Remove only maximum comparison | 101; otherwise accepted |
| Payload | Valid magic, valid complete HTTP JSON header, zero payload bytes | Replace `>=` with `>` | 101; otherwise accepted |
| UTF-8 | Valid framing and payload; invalid UTF-8 header byte | Substitute `{}` only on UTF-8 error | 101; expected UTF-8 error changes to response-header error |
| JSON valid path | Valid framing and valid HTTP JSON header | Replace parsed header with {} | 101; valid envelope becomes a response-header error |
| HTTP type | Valid framing/JSON/nonce/encoding, wrong type | Disable type clause alone | 101; otherwise accepted |
| Nonce | Valid framing/JSON/type/encoding, numeric nonce | Disable string check alone | 101; otherwise accepted |
| Encoding | Valid framing/JSON/type/nonce, raw encoding | Disable gzip clause alone | 101; otherwise accepted |

Error classification is itself observable: zero-length and UTF-8 controls pin
the correct decode failure rather than claiming their removal accepts malformed
bytes. The former JSON-throwing guard was removed in the PR 7216 repair: TS
jsonParse returns null, so malformed JSON must reach response-header validation.
There is no independent JSON-rejection predicate left to claim; the replacement
control proves valid-header parsing, not a separate rejection guard. Every
retained guard mutation fails independently; none requires another mutation. TypeScript's separate
`!message.data` check is intentionally consolidated into the encoding predicate:
missing, null and false data cannot carry `data_encoding == "gzip"`. Separate
corpus entries pin their rejection. A second data-presence predicate would only
be redundant here. The gzip-only test was renamed `rejects_corrupt_gzip_payload`
to remove its misleading claim to exercise binary-envelope validation.

## Five previously undispatched invariants

Each single mutation produced **101 with an actual test assertion failure**:

| Invariant | Mutation | Witness |
| --- | --- | --- |
| Close invalidates old receive work | Remove generation increment | `old_receive_error_cannot_close_the_replacement_socket` |
| Ping-send failure keeps monitor alive | Change ping-failure `continue` to `return` | `ping_write_failure_reconnects_and_keeps_idle_monitor_running` |
| Binary magic validation | Disable only TSB1 comparison | Binary corpus above |
| GRVT top-level proxy bypass | Disable only GRVT arm | `grvt_frame_bypasses_proxy_but_other_venues_do_not` |
| Exactly 16-digit raw token protection | Change scanner threshold to 17 | `raw_integer_scanner_protects_exactly_nested_sixteen_digit_tokens` |

The scanner witness checks scanner output directly; response normalization
cannot conceal a wrong threshold. The generation witness uses an old receive
future after a replacement socket is installed; reconnect count alone cannot
substitute for the generation guarantee.

## Relay contract proof

`RelayWebSocket::new(host,url)` is unchanged. The additive options carry static
or fresh auth headers, protocols and authentication-only `headers_required`.
Fresh providers stay lazy until each inner tunnel dial. Two replacement relay
instances reuse one provider and produce token-1 then token-2. Recorded dial URL
is the parsed outer tunnel URL; caller options never reconstruct the upstream.

| Independent control | Result |
| --- | --- |
| Skip fresh provider evaluation at dial | 101, fresh-header witness fails |
| Disable required-auth construction preflight | 101, expected Result error fails |
| Remove only lazy-dial preflight | 101, capabilities changed Native→Browser; error must precede provider/token/dial |
| Remove wasm overlay inertness only | 1, compiled wasm invokes the forbidden fresh callback |
| Remove wasm required-auth preflight only | 1, compiled wasm's required-refusal assertion fails |

`yarn warning:wasm` executes the compiled Rust probe in Node via the locked
wasm-bindgen CLI, not a host-side reimplementation. It deliberately declares
native host capabilities so wasm inertness is independent of browser tagging.
Nonrequired native/browser tests also measure protocol forwarding/ignoring,
sealed Origin/UA/HTTP1.1 and absence of a dial on construction refusal.

The inherited BTreeMap host header ABI cannot express ordered application of
duplicate HTTP field names, though differently cased keys are representable.
Removing case-conflicting caller Origin/UA keys is therefore forced by that ABI
and preserves the WAF policy deterministically. Restoring conflicting variants
could let caller casing shadow required Origin through downstream normalization.
The constructor manifest declares this and unrepresented arbitrary `unknown`
Node options; auth is never hidden in an opaque downcast channel.

## Other witnesses and practical limits

Configuration tests cover shallow merge, defaults, explicit optional clears and
callbacks reaching actual fake tunnel auth and clock handling. Latency tests
exercise known/used-domain enrollment after successful sends, one-second ticks,
strict `> 10_000` staleness, stop/dispose, error rejection and nullish defaults.
Custom-parser tests exercise both request and raw-response paths, forced region,
throw fallback, original request context and early/final deadline checks.

Simulation counters distinguish a request **handed to a dependency** from its
I/O future **actually polled through the endpoint operation**. An expired call
is handed once but reaches that operation zero times. These are deterministic
fake-host observations, not claims about packets reaching a live venue. No live
network venue test is part of this chunk.

The scoped suite retains independent caller timeout/cancellation, shared refresh,
weak regional cache, inbound close metadata and typed shared frame assignments.
The legacy RestAdapter ABI has no arbitrary Axios keys, JS prototype/originalError
objects, DOM prevention/propagation or Blob identity. HOW snapshots are cloned;
latency domains must be strings. Direct-only simulation hosts are now accepted;
real tunnel dials still validate capabilities. Representation limits remain in the
manifest/README rather than being represented as full TypeScript parity.

Additional single mutations all failed with **101 and assertion failures**:
reconnect token reset, rpc_version 4→3, deadline primitive precheck, dead relay
socket release, latency strict-staleness boundary, latency stop, error-response
filter, cached default, host buffered amount, request parser, response parser,
forced region, early deadline, final deadline, simulation selection, sensitive-key
redaction, string limit, array limit, object limit, array depth and object depth.
`how-behavior-controls.py` records each exact mutation and its filtered witness.
The array-depth mutation initially passed the object-depth corpus; adding a
nested-array witness made it fail independently. A request-parser mutation that
only triggered an unused-variable compile error was rejected as proof, corrected
to compile cleanly, and then failed the parser assertion. Neither false control
was counted as evidence.


## Taxonomy rulings and independent controls

The four findings are fixed: simulation bypasses constructor-only tunnel checks;
Bad-auth joiners share one non-timeout failure while preserving independent
deadlines; raw parsing reproduces the quoted-string corruption as outbound
arg.data = null; the exact staging flag selects both HOW and relay endpoints.
The raw parser retains a divergent row for the existing serialized-byte input
boundary, which cannot accept dynamic Axios objects or DOM FormData.

Updated taxonomy: **100 rows = 80 ported + 20 divergent + 0 pending**.
Of the 20 divergent rows, **10 FORCED, 9 CHOSEN, 0 CORRECTED, 1 MIXED**:

- FORCED: createWebsocket, handleWebSocketError, handleLatencyResponse,
  makeCloseEvent, makeErrorEvent, sanitizeForTradeTunnelLog,
  formatTradeTunnelErrorMessage, toArrayBuffer, parseWebsocketData,
  createAxiosHttpOverWebsocketAdapter. These are existing typed host/value/byte
  boundaries; the last two no longer include the parser correction or simulation
  restriction.
- CHOSEN: once/off/emit (one event-ownership decision), connectAndSubscribe,
  clearHandshakeTimer, isTradeTunnelVerboseLoggingEnabled, logTradeTunnelVerbose,
  getHOWWebSocketForExchange, getHOWConfig. The previous justifications stand.
  Wasm verbose logging is explicitly an implementation simplification, not
  necessity: a JS bridge is feasible, with no parity argument for omitting it.
- MIXED: RelayWebSocket.constructor retains forced deterministic policy-header
  precedence under the inherited BTreeMap ABI, plus the accepted omission of
  arbitrary unknown Node extensions. This row is not relabeled wholly FORCED.

Zero pending does not mean zero acknowledged omissions: arbitrary unknown Node
options and wasm verbose logging remain accepted omissions. No public API was
added in these four fixes: 433 indexed items, 55 added versus base, zero added
reexports, all 55 claimed non-pending, none exempted.

Reproduce the new controls with
python3 sdk-rs/safe-cex-transport/scripts/how-ruling-controls.py.
Each mutation runs alone, restores its source, and requires an assertion failure:

| Mutation | Independent witness | Exit |
| --- | --- | --- |
| Restore simulation constructor capability rejection | Direct-only Bun/native hosts must simulate, later real dial fails closed | 101 |
| Suppress published refresh failure | Eight long-deadline joiners share one error, one short caller times out, later call retries | 101 |
| Skip raw regex rewriting | Emitted frame contains null rather than the original object | 101 |
| Remove rewriting from live TS parser | Correction-specific TS null assertion fails | 101 |
| Remove FEFF from private whitespace | Live Node oracle | 101 |
| Add NEL to private whitespace | Live Node oracle | 101 |
| Disable staging selector | Child processes compare unset/0/1/true with live TS and inspect HOW/relay dials | 101 |
| Route relay through production constants | Generated routing's private relay URL witness | 101 |
| Disable wasm build-environment selector | Compiled wasm endpoint assertion with staging=1 | 1 |

The TypeScript oracle compares the shared whitespace shim against every
Unicode scalar (1,112,064) using live Node trim. FEFF is included and NEL excluded.
The transport source audit found no bare str::trim, trim_start, trim_end,
char::is_whitespace or split_whitespace on any input.

Consolidated onto safe-cex-num after integrating 9a45f142cd (PR 7219).
The private character set and direct ryu-js dependency are removed; trim-start
and Number-to-String use the shared primitives. No consolidation blocker remains.

Read-only sub-agent review found no concrete blockers in the four fixes.
Tests use fake hosts; outbound witnesses establish handed frames, not live venue
execution. Native staging is captured once at first endpoint use; wasm staging
is captured at build time, matching browser bundle substitution.

### Shared JS primitive consolidation after PR 7219

Integration 9a45f142cd merged without conflicts into the HOW branch. Transport
tests with --include-ignored: 110 passed, 0 failed, 0 ignored (104 tests and
6 doctests). Plain clippy --all-targets, wasm build, invention/status gates
(6901 rows, 457 public items), and layer gate (20 tests) passed.
The nine ruling controls replayed independently: eight native assertion failures
at 101 and the staging wasm assertion at 1. The FEFF and NEL controls now mutate
the consumer's shared trim-start application separately; both are still red.
The live Node comparison across all Unicode scalars remains in the transport
oracle. No whitespace-set disagreement or behavior change was observed.

### PR 7216 inventory limitation

The 100-row / zero-pending claim covers the extractor's current inventory only.
It misses export-let declarations: howWebSocketSG and howWebSocketJA have no
rows yet. Their Rust regional registry is weak and lazy, whereas TS eagerly
constructs process-lifetime module singletons. Collection/recreation resets auth
and latency state; this lifetime divergence must be claimed on those rows when
the separately owned extractor correction lands. No zero-omissions claim applies.

The reviewed extractor correction 69fd5519a0 is now imported as 6a3b4a114f.
The inventory is 6904 rows, including both exported HOW singletons; their new
rows explicitly declare weak/lazy lifetime and auth/latency reset on collection.
The previous limitation above is historical, not an outstanding hidden tail.

### PR 7216 ported-row repair and shared parser

This section supersedes the earlier 80/20 and 100-row summaries. HOW now has
102 rows: 71 ported, 31 divergent, zero pending. The two singleton exports are
included. The new narrow json-parse root adds two separately accounted divergent
rows, so the total extractor denominator moves from 6904 to 6906. These parser
rows disclose the UTF-8/owned-JSON boundary (including non-finite numbers becoming
null); they are claimed, not exempted. Zero pending is not zero omissions.

The branch repairs closed-frame ping/pong substring drops and inner ready-state
checks, orphan latency scheduling, relay error interpolation, auth exhaustion
isolation, terminal simulated close, browser base64/BOM/gzip/null decoding,
omitted close status, parked handshake deadline cancellation, buffered bytes
after fail, disposal request deadlines and retained identity, and explicit
undefined config resets. The direct response reason remains optional; HOW
deliberately discards even a populated tunnel status_text. Bybit's direct reason
phrase collision with health patterns is inherited TypeScript behavior.

The live TypeScript latency lifecycle witness corrects one review claim:
repeated start creates two chains and stop cancels only the last handle, but
after dispose the orphan fires once and does not reschedule (one timer to zero).
Both the Rust behavior and the live TS bug are pinned. Restoring idempotent
start in Rust and fixing it in TS independently fail; likewise for closed-send
ping substring suppression. An actual host send error can still throw: the
silent path applies to sockets whose readyState is not OPEN.

Remaining differences are explicit, not silently recategorized as parity:
- createWebsocket and relay construction are MIXED host representation and
  chosen fail-closed policy. BTreeMap cannot preserve ordered application of
  duplicate HTTP field names; dropping case-variant conflicts is the chosen
  policy-preserving remedy, not a claim that the map cannot store both keys.
- Owned event/value/error representations omit dynamic DOM/JS properties.
  Not-ready errors preserve the name but not ad-hoc JS error fields.
- Weak/lazy regional lifetime, cloned config/maps, host credential fallback,
  finalizer ownership, and logging remain declared choices or mixed boundaries.
  Arbitrary unknown Node options, arbitrary TinyEmitter events and wasm verbose
  logging remain acknowledged omissions.
- Preparation serializes complete auth work rather than just a refresh promise.
  A joiner can start its own retry counter later and make more auth attempts,
  bounded by its own complete-request deadline. This path also prepares orders:
  extra attempts send only rpc.auth before the HTTP frame is built or sent.
  They cannot replay an order. Owner exhaustion is not inherited by joiners.
- Core and relay preflight errors are distinct Rust boundary types. The relay
  constructor explicitly claims the shared private TS option/provider shapes;
  core owns its own base.ws error rows.

The shared public contract is
safe_cex_transport::how::big_int_as_string_json_parse(&str) -> serde_json::Value.
HOW raw parsing calls it directly. It includes rewrite, parse, and null fallback;
the scanner has no in_string skip. The older scanner seen on the core branch
predates adc10e5597; tree skew was not a regression. The ordinary json_parse
export is also represented. Public-API witnesses pin quoted-string corruption,
invalid input, 16-digit protection, ordinary binary64 rounding and exponent
values. The live TS oracle checks both exported parsers.

Behavior controls run one mutation at a time and require exit 101 plus a test
assertion failure; compile-only rejection does not count. The JSON-throwing
predicate is retired, not relabeled independent: TS jsonParse returns null and
response-header validation rejects it. The replacement positive-header control
tests parser use, while every retained envelope guard/subcondition retains its
own independent malformed witness.

Final repair validation:
- Transport CI-equivalent commands (--locked --all-targets -- --include-ignored,
  then --doc): 118 tests + 6 doctests passed, 0 failed, 0 ignored.
- Workspace fmt check, plain strict transport clippy --all-targets, wasm build
  and compiled wasm runtime probe: exit 0.
- Extract --check: no added/orphaned/changed manifests. Status/invention and
  report: exit 0, 6906 rows and 461 indexed public Rust items.
- Layer gate: 20 passed. Extractor suite: 36 passed; the corrected JSON-root
  path-mapper witness was rechecked separately, 1 passed. Extractor typecheck
  and lint: exit 0. Imported parity crate tests: 66 passed, 0 ignored.
- Validation remains package-scoped; no full monorepo run or live venue I/O.

| Replay runner | Results |
| --- | --- |
| how-review-controls.py | 19 independent native assertion failures, each 101 |
| how-invariant-controls.py | 16 independent native assertion failures, each 101 |
| how-behavior-controls.py | 21 independent native assertion failures, each 101 |
| how-gate-controls.py | 9 rejected mutations, required diagnostic checked |
| how-relay-controls.py | Native lazy preflight 101; two wasm runtime controls 1 |
| how-ruling-controls.py | Eight native assertions 101; staging wasm assertion 1 |

These are 77 control executions across the runners, not 77 unique invariants:
the gate test mutation intentionally repeats the magic-byte invariant. Review
controls were run as the original 15, two live-TS mutations, tunnel reason
passthrough, and ordinary shared-parser rounding; the script can replay all 19.
The ruling replay was repeated after extracting the public parser. The root
removal control was repeated after fixing the new test's path-mapper call.

The gate controls distinguish fmt Diff, Clippy unused variable, failed runtime
test assertion, wasm compile_error, orphaned extractor symbols, missing layer,
unclaimed public item, report refusal, and JSON-root assertion failure. Status
is checked independently and must contain no invention diagnostic. An initial
run stopped on an invalid TOML apostrophe escape in a disclosure; after fixing
the note, all nine controls replayed with their intended diagnostics. Typecheck
also caught a missing argument in the new root test; the corrected test invokes
the actual path mapper to check reserved-prefix collisions, rather than relying
on a parser throw for an unrelated reason. Neither initial failure was counted
as a successful negative control.

The shared parser's remaining UTF-8/JSON-value boundaries, weak singleton
lifetime, host callback fallback, event ownership and serialized auth timing are
declared divergences. Their notes are not evidence of complete JS/DOM parity.

## Integration merge for verification repairs

Merged integration `311678087e` into HOW `97dee8b17a`. Both parents and the
merged workflow have 52 named CI steps; none were removed. Extractor conflicts
were resolved per hunk, retaining the JSON parser root test and the integration
scope audit. Integration layers.json and candle-cache LoadedChunks signatures
were retained. Extractor: 36 passed, zero skipped. Transport package tests and
six doctests passed, zero ignored. This validates the merge, not the pending
verification findings.

F1 verification repair: both frame paths call shared json_parse before typed
decode. Wire witnesses 9007199254740993, 1.50, 1e3 serialize as JS binary64
9007199254740992, 1.5, 1000 in the public latency entry. Status 200.0
normalizes to u16-compatible 200. Text and binary bypass mutations independently
failed runtime assertions at 101 (how-review-controls text-frame-rounding and
binary-frame-rounding); restored targeted tests pass.

Post-merge manifest inspection found Git auto-merged duplicate pending SG/JA
rows beside their existing divergent mappings. Removed only those duplicate
blank rows; retained both lifetime disclosures. CI step counting alone cannot
catch an auto-merged semantic duplicate.

F2: relay becomeOpen now drains before sending and propagates a send failure
without closing, preserving the TypeScript bug. Native failure-path and live
TypeScript witnesses pass; restoring catch-and-fail independently fails at 101.

F3: token rejection and auth-send rejection both emit exactly proxy token
unavailable. Both independent mapping-removal controls fail assertions at 101.
The first token control initially survived because the fake already returned
that same message; replacing its rejection with distinct provider text made
the witness discriminating. That initial surviving run was not counted.

F8 taxonomy correction: validateAndSend diagnostics are CHOSEN omissions.
TransportError is owned by this port and can carry a structured variant; the
Host(String) representation cannot justify FORCED. No substantiated reason
that mirroring would be worse has been found; the row now says so plainly.
No error enum change overlaps the core agent's active boundary migration.

F6: heartbeat send errors are consumed by onMessage, matching its TS catch.
A pending request retains its timeout and socket generation. The positive
witness passes; propagating the heartbeat error independently fails at 101.

F4/F7/F9 disclosure corrections: summarizeTradeTunnelUrl declares the same
UTF-16 split-surrogate replacement as the sanitizer, pinned with a prefix of
a plus 250 astral characters. syncServerTime and onMessage now disclose the
chosen f64 callback narrowing and host fallback at both implementing rows.

Zero pending is scoped to packages/http-over-websocket/src. Two adjacent rows
under packages/safe-cex/src/httpOverWebsocket remain pending: initializeHOW
and getServerLatencyForExchange. This repair does not claim those rows.

Dependency warning correction: the complete public parser wrapper commit is
04bb3c6182, not d10ee51cbc. Scanner repair adc10e5597 is its ancestor; the
wrapper is never served by the old scanner in this branch ancestry. d10ee51cbc
was the old transport tree inherited by core. Consume the branch by merge,
including its scanner prerequisite, rather than copying a wrapper onto that
older tree.

## Verification repair final replay (after ae9e2e7bd6)

HOW scope: 102 rows, 68 ported / 34 divergent / zero pending. Adjacent safe-cex
HOW rows remain pending as named above; zero pending is not zero omissions.
F8 diagnostic fields and F7 dynamic clock inputs remain chosen omissions, not
forced constraints. F4 retains the disclosed lone-surrogate replacement.

F5 now emits top-level type/rpc_version, optional raw fields, optional bypass,
nonce and arg in source order; four raw/bypass combinations pin the emitted
frame. The arg remains the typed path-shared server struct. Moving nonce/arg
before optional fields independently fails at 101.

Sequential replay on the repaired tree:
- how-review-controls.py: 28/28 runtime assertion failures at 101, including all
  19 prior review controls and nine new verification controls.
- how-invariant-controls.py: 16/16 runtime assertion failures at 101, including
  the five legacy invariants and each binary-envelope guard/subcondition. The
  positive JSON-parser control now targets the shared json_parse call.
- how-gate-controls.py: 9/9 rejected mutations with named diagnostics: fmt Diff,
  denied unused variable, runtime assertion, wasm compile_error, orphaned TS
  symbol, missing layer, unclaimed Rust public item, report refusal, JSON-root
  assertion. Status remained isolated from invention.

These are 53 control executions, not 53 distinct behaviors. The other unchanged
behavior/ruling/relay runners were not rerun this round; their earlier evidence
above is not relabeled as fresh execution. All mutation files were restored.
The restored tree then passed workspace fmt, plain package clippy all-targets,
126 package tests (--locked --all-targets -- --include-ignored), six doctests,
wasm build, extractor check (zero changes), status/invention, report, and the
JSON-root test. Zero failed or ignored tests. Inventory: 6906 rows, 462 indexed
Rust items. Earlier in this pass: CI wiring 99 passed, extractor suite 36 passed,
layer check classified 37 crates, progress report succeeded. No live venue I/O.

Implementation commits: F1 67ce40647c; F2 849ca9c08a; F3 2dfd407e0f;
F6 892c82c219; F5 81d47c5697. F8 taxonomy e4a6d387d1. F4/F7/F9 and
wrapper-ancestry disclosure share 1e5b402ac7. Integration merge bd16601229
retained all 52 named steps from each 52-step parent; follow-up 5fd5e269d5
removed only Git's automatically duplicated blank singleton rows.

Overflow-before-interpretation repair: both public JSON parsers shared an early
Infinity-to-null projection in normalize_json_numbers. Finite numbers still
collapse through binary64; overflow now retains the arbitrary-precision Number
lexeme for shared JS String/Boolean coercion. No caller flag selects correctness.
The bigint rewrite path had the same defect and is covered by the same fix.
The external parser_overflow test executes both actual TypeScript exports and
compares String/Boolean BEFORE JSON serialization can hide overflow as null.
It includes negative and nested overflow, finite rounding, decimals and exponents.
Independent plain-parser and bigint-parser null substitutions each fail this
named assertion at exit 101. Scoped transport tests: 134 passed, 2 ignored
(existing explicit oracle gates), including 6 doctests; strict all-target clippy
passes. This is local evidence, not an integration CI claim: GitHub run lookup
still fails certificate verification (OSStatus -26276).

Decoder-family follow-up: codec text/binary headers call json_parse; HOW's other
body/response normalization sites share the fix. Relay handshake and signing
try_parse_json directly retain arbitrary-precision numbers; retry parsing only
inspects string fields. The signing helper's finite normalization is a separate
owner concern, not claimed by this overflow repair. Retaining overflow revealed
that HOW send must use JS JSON serialization at the actual outbound boundary.
It now does, with ordinary/raw body and query-param assertions; restoring serde
serialization independently fails at 101. Response byte handoff retains lexemes
for downstream venue interpretation. Full scoped transport replay: 135 passed,
2 existing explicit-oracle tests ignored (6 doctests included).

## Signed-zero sibling of overflow preservation

Live-source parser witnesses compare binary64 bits BEFORE JSON serialization,
plus String, Boolean and final shared JSON stringify. Both actual exports cover
root/nested -0, -0.0, -0e10, negative underflow, positive zero/underflow, a negative
subnormal, ordinary finite normalization and the bigint nested-integer branch.
Before repair, both independently failed the named sign assertion at 101:
0000000000000000 versus source 8000000000000000. The normalizer now retains a
negative-zero lexeme rather than sending it through Number-to-String, which
intentionally spells both zeros as 0. No flag or venue-local parser was added.
Signed-zero and overflow tests pass (3 tests, zero ignored); host and wasm
all-targets strict Clippy each pass separately. Changed parser files pass fmt.
Clippy first exposed a LOCAL MERGE ARTIFACT in the heartbeat test: our merge
combined creation-site and assertion-site leaf_error wrappers. This is not a
broken integration base: 93bcc22528 has only the assertion-site wrapper.
f2f1122a29 removes the duplicate in this branch; the unchanged heartbeat timeout
and no-disconnect assertions pass. Earlier reporting called it inherited; that
attribution was wrong and is corrected here.
Follow-up check passed using the exact unmodified git-show tunnel.rs from
93bcc22528 under this branch's library, then restoring our file: cargo check
-p safe-cex-transport --tests --locked --offline --jobs 2. This is file-level
validation of the base test, not a separate clean integration-tree build.

## Safe-cex wrapper dependency inventory

At integration `dce4c1948ae31829d8921029d417093144cff286`, enumerating manifests
by both source-tree prefixes gives 12 files / 104 rows: 68 ported, 34 divergent,
2 pending. The two pending safe-cex wrapper rows are distinct from the nine
package HOW manifests. The earlier brief's 49 pending is stale.

`safe-cex-how-wrappers.cjs` executes both actual source wrappers with explicit
module-dependency spies. On arm64 Node 24.20.0 it pins two configuration calls,
eight clock forwards and six latency paths. Omitting import initialization,
omitting the domain guard and forwarding the wrong domain each execute and exit
1 at their named HOW-BRIDGE assertion; these are source mutations, not Rust
compiling controls. The wrappers still lack production Rust operations and the
shared dependencies inventoried in their pending FINDING notes.

The real Yarn entrypoint's `parity:extract` exits 0: zero updated manifests,
zero added/orphaned symbols. No Rust code changed, no Rust gate was rerun, and
this source-only evidence makes no wrapper parity, full integration or CI claim.
