# GRVT remaining local command terminals

Plan-first base: actual49f7d1857b66fc14f5164c8ca82ce42d48a53c1d.
The overseer granted two lib terminals: migrate the reachable generic Unsupported
refusal and separately delete the provably dead private-phase-pending group.
No production changes precede original controls and native/core review.

## Native scope and reachability

The generic Unsupported GRVT command arm receives CancelOrdersWithSymbols,
FetchMaxLeverage, FetchStpMode and SetStpMode through genuine engine entry.
AmendOrderWithSymbol is refused by the engine because amend_with_symbol=false;
options commands are refused before entry because options=false. FetchAccount
and FetchOrderBook already return through identity and BookQuery dispatch.
The originals must establish these boundaries rather than invent direct entry.

The private-phase-pending group lists PlaceOrders, CancelOrders, AmendOrder,
CancelSymbolOrders, SetLeverage, SetMarginType, SetPositionMode, FetchLeverage and
Query. Every variant has an earlier unconditional dispatch and return. Delete
that arm under #2074, retaining exhaustive match coverage in the generic arm.
Deletion grants no new behavior or managed authority. Genuine safe local probes
must show those earlier native paths run, never the private-phase diagnostic.

The native generic error is a raw string, with kind/outcome/code/hint absent.
Read versus Mutation classification follows the command, never the message.
Use keyless Read for FetchMaxLeverage/FetchStpMode and keyless Mutation for the
cancel and setting refusals, 20s/Never/no pacing, only inside the local arm.
Shared admission rejection owns the single terminal; no numeric fallback or
credential/queue/signing work may run. Context admission and caller receipt,
capacity, duplicate ownership and finite clock precede the native diagnostic.
CALLERS must disclose that precedence and label the dead-arm change DELETION.

Overseer ruling: accept the established shared pre-release contract, Read outcome
None / Mutation NotSent. The CancelOrdersWithSymbols and SetStpMode native
outcome None becomes NotSent after genuine admission, with text and kind
unchanged. CALLERS must explicitly cite this precedent. The original native
metadata assertion is superseded only for these Mutation outcomes; no shared API
change, fake Read or local projection override is permitted.

## Inventory and preservation

Remove only the generic Unsupported and private-phase-pending singleton hashes
from the GRVT lib key. This is two lexical terminals: one adoption and one dead
code deletion. The catalog PR independently removes two other lib sites; do not
claim or prune those here. Preserve foreign inventory raw bytes/order and all
signing/credential maps. No account/history/identity/trading/queue/signing/host/
shared engine source changes. Compose after the catalog without changing its
managed delivery or retirement paths. Pending.context remains login-only.

## Original controls

Use a thin Venue delegator over real Grvt::on_command with native capabilities.
Startup seeds only observable market/hedged prerequisites, with no fake terminal
or simulated HTTP. Count forwarded calls to distinguish actual engine rejection
from venue refusal. Four native generic cases pin message/kind/outcome/code/hint,
no I/O/Store changes, keyless/public-only/configured modes and all exact paths.
Pre-entry symbol-amendment refusal must have zero forwarding; a reachable ordinary
command proves the delegator is active. Safe invalid commands for all nine dead
variants must reach their earlier real native entry and never the dead message.

For each reachable case, full256 genuine Command Read owners must produce one
shared capacity failure, no I/O or Store effects. Recover by retiring the exact
first peer, repeat the native result, and validate all255 remaining peers. Pin
NaN external Event, checked zero caller wait and duplicate-owner retention.
Shared Read failures carry no outcome; Mutation admission failures carry NotSent.
Original text/kind/code/hint assertions remain unchanged; the overseer explicitly
superseded only the two Mutation outcome assertions as described above. No missing-method/setup failures count as REDs;
disclose suffixes gated by the first capacity or clock assertion.

Commit plan before compiling originals; independent native and core CLEAR are
required before source. Scoped Cargo uses CARGO_BUILD_JOBS=4 sequentially. Only
intended freeze heads are pushed, with one PR Rust run and no duplicate dispatch.


## Original execution checkpoint

The new target compiles and executes18 controls:8 intended assertion REDs and
10 GREEN, zero ignored. Four capacity cases fail at native kind None versus
shared Internal; four NaN external-entry cases fail at None versus InvalidInput.
No fixture/setup/missing-method failure is counted. Recovery and255-survivor
suffixes remain gated after the first capacity assertion.

Native controls independently prove all four generic commands reach GRVT and
retain exact raw text with no kind, outcome, venue code or hint, including both
Mutation cases. Configured/public-only variants keep this metadata and no I/O
or Store effects. Checked zero-wait and duplicates preserve original ownership.
Symbol-aware amendment is refused before venue entry; a subsequent ordinary
Read proves forwarding is wired. Every one of the nine dead-group variants
reaches its earlier native handler and never emits the private-phase diagnostic.

Execution confirmed the original Mutation metadata mismatch: native cancel and
setting refusals have outcome None. The overseer then accepted shared Mutation
NotSent as the precise pre-release disposition. Supersede the native helper's
outcome assertion with command-classified Read None / Mutation NotSent, keeping
every text/kind/code/hint assertion. This produces additional genuine assertion
REDs on unchanged production; record their exact count before source.

Revised original checkpoint after the ruling:18 execute12 intended assertion
REDs/6 GREEN/0ignored. Four added REDs are precisely the two native Mutation
cases and configured/public-only loops reaching their Mutation outcome check
(None versus NotSent); the original eight capacity/NaN REDs are unchanged.
The prior native-None evidence remains immutable at2161420eaec. No production,
fixture setup or non-outcome expectation changed during this supersession.



## Production checkpoint

The new unsupported_command helper takes only the command-classified operation.
It derives owner identity from genuine dispatch through admit_http_command,
uses keyless20s/Never/no pacing and returns on shared refusal. fail_context
keeps native raw text and absent kind/code/hint, Read None and approved Mutation
NotSent. No numeric ID is borrowed. The earlier dispatch blocks are unchanged;
the dead private-phase arm is removed and exhaustive generic coverage retained.

Exactly two singleton lib hashes disappear: bfcdb38d1528a6090a59eb123fc288423fd2ce7a45cf8179d786b7dafb8a5b2c
and d378cf6b03a38047ea106b9344672d153498eb2f0f1aa48f79efcdfeb037c3f9.
Both are1->0, lib8->6 on this base; the separate catalog owns different hashes.
Foreign raw inventory bytes/order and all other maps stay unchanged.

All18 revised originals now PASS, including the earlier gated recovery and
255-peer validation suffixes; library7 and trading42 also PASS (67total).
No existing fixture changes or further original assertion changes were needed.
The initial helper compile found an unused numeric ID parameter; it was removed
before the successful run and is not counted as an assertion RED. Source review
and intended-head CI remain required before READY.

Strict scoped GRVT all-target Clippy -D warnings, rustfmt and full bans/ratchets
PASS sequentially with CARGO_BUILD_JOBS=4.
