# Ten approval and spot cancellation rows: measured receipt

Code SHA: `650a576d5548dde3e5dae8c9e616aabb55858bb0`.
Actual parent built on: `d3e8141626c0ab4c235b3816a986144961349554`, the requested
integration head. No unrelated unreviewed feature ancestor was used.
This receipt is an evidence-only child. The attached [gate exit file](GATE_EXITS_650a576d55.json)
binds the measured results to the code SHA; later merge-down preservation is
reported explicitly at delivery, rather than attributing old exits to a new SHA.

Independent Python3.12 tomllib comparison with that parent moves **10 rows
pending->ported, 0->divergent**: 99/106/326 becomes **109/106/316**. All531manifest
rows were counted; only the ten named rows in the exit file changed status.

Five builder approval constants and one agent EIP-712 type table match actual
module export members/field order. Default fee/rate are derived from the existing
builder constant and helper. These are native constant value models; signing,
submission, browser composition and module initialization timing are not claimed.
`generate_agent_wallet_name` performs four fresh fallible host random operations,
including JS floor/charAt behavior and error identity. `has_ethereum_wallet`
ports the guard over explicit host bindings: availability/type read, then a
fresh second window lookup plus ethereum getter and JS ToBoolean. The host
supplies those engine operations; no browser implementation is claimed.

The closed cancellation-list declaration matches core exactly. Spot schedule
is deliberately separate: spot `time?: number | null` maps to absent `None`,
explicit null `Some(None)`, or numeric `Some(Some(f64))`; the perpetual source
has `time?: number` and must continue to reject explicit null. Strict-map and
JsNumberInput ingress preserve signed zero and raw numeric overflow in the spot
model. Newly exercised decoder entry point is serde_json::from_str only.

Actual-source oracle floors: six constants and86cases:50name,18wallet,18cancel.
Name enumeration stops at index49 (36alphabet-slot cases, six additional number
cases, eight throwing getter/call cases); wallet at17; cancellation at17.
The18cancellation cases are checked by the strict actual TypeScript compiler
(nine accepted/nine rejected), including structural rejection, optional-null and
number grammar. These are corpus floors, not exhaustive accepted-set proofs.
One whole source invocation runs per native test binary, not per data case.

## Gates at the code SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid
  --all-targets --jobs 2`: **0**, finished successfully.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  **0**, output read:7,341symbols/512files; `Would update 0 manifests; 0 added,
  0 orphaned symbols.` Writer ran after manifest edits and reported `Updated 0
  manifests` before the code commit; no array was hand-written in a manifest.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
  node scripts/approval-leaves.cjs`: **0**.
- Python3.12 `scripts/audit-row-dispositions.py --check ROW_DISPOSITIONS.json`: **0**.
- `python3 scripts/approval-leaves-control.py`: **0**, requiring the measured
  Cargo mutation exit **101** and restored Cargo exit **0** with **5passed/0failed**.
  Both Cargo commands were the scoped `--test approval_leaves --jobs 2 -- --nocapture`
  run with SAFE_CEX_TYPESCRIPT_PATH set to the real source compiler.
- Restored source/tests diff against code SHA: **0**; all four temporary production
  file mutations were byte-restored before the restored test.

An initial uncommitted check exited101 for a missing numeric-output marker,
which was added before the successful check. A queued precommit Cargo test was
started prematurely and immediately terminated before acquiring the build lock;
its exit143 is not a gate. Those precommit results do not cover the code SHA.
Every final Cargo run above was completed sequentially.

## Discrimination

One combined production mutation adds1to the default fee (also changing its
derived rate), changes both domain names and first EIP-712 field names, makes
three instead of four random calls, flips the wallet predicate result, swaps
both serde metadata labels for cancellation a/o, and collapses explicit schedule
null into absence. All five native tests failed with all ten required assertions.
Each constant root panic is caught so all six constant assertions execute.
- `APPROVAL native DEFAULT_BUILDER_FEE_TENTHS_BPS bits`.
- `APPROVAL native DEFAULT_BUILDER_FEE_RATE text`.
- `APPROVAL native HYPERLIQUID_BUILDER_DOMAIN name`.
- `APPROVAL native HYPERLIQUID_BUILDER_DOMAIN_TESTNET name`.
- `APPROVAL native APPROVE_BUILDER_FEE_TYPES field name 0`.
- `APPROVAL native APPROVE_AGENT_TYPES field name 0`.
- `APPROVAL native generateAgentWalletName name case 0`.
- `APPROVAL native hasEthereumWallet value case 1`.
- `APPROVAL native CancelOrderRequests asset bits case 1 item 0`.
- `APPROVAL native ScheduleCancelAction null case 9`.

Concrete measured pairs include fee11vs10 (bits4622382067542392832vs4621819117588971520),
rate`0.011000000000000001%`vs`0.01%`, name`Tealstreet aaa`vs`Tealstreet aaaa`,
wallet case1truevsfalse, cancellation case1asset91.25vs17.5
(bits4636121564843081728vs4625619029774565376), and schedule case9`None`vs`Some(None)`.
Both domain names and the first field names carry a distinct `#CONTROL` suffix.
The a/o swap is symmetric in serialization/deserialization metadata, so a
roundtrip could hide it; the test reads native fields and catches it directly.
Native assertions precede any external wire assertion (this suite has no wire
assertions). The production numeric ingress internally checks its serializer
capture; the independent native field test supplies the missing symmetry check.
No survivor occurred. Restored5tests passed after all source files were restored.

Read-only sub-agent review found no correctness blocker, independently measured
ten moved rows and the86source cases, and requested the now-applied second-window
lookup contract clarification. Reviewer ran no Cargo and made no edits.

Not run: full workspace builds/tests, full crate runtime suite, host clippy,
wasm clippy/build, live venue/browser tests, or CI. Cardinality sweep stays **OPEN**.
The13original FINDINGs remain individually named in ROW_DISPOSITIONS.json.
The old four-class audit was refreshed for census only; no old row was promoted.


## Post-merge gate results and adapter correction

Integration `55d8addaf74e5ff2c2da80ab19a03109b6fb9476` was merged into
`91d8bd971728da36c8b1d0045ffbedd60527e619`. Venue source/tests were unchanged,
but shared dependency inputs changed, so scoped checks were rerun. The first
merged all-targets check exited **101**: the shared ExchangeRuntime trait added
monotonic_now_ms, missing from the venue's test Clock. A five-line adapter fix
uses an independent stationary monotonic clock; its wall-clock jumps and
reentry hooks remain separate. The adapter forbids spawn and rejects sleep;
no elapsed-scheduler behavior is claimed.

At fixed code SHA `f9080e6afb68741f36f20907e3909f63b8eddfeb`:

- Scoped all-targets Cargo check: **0**.
- `--test approval_leaves --jobs 2`: **0**, **5 passed**.
- `--lib exchange::tests:: --jobs 2`: **0**, **8 passed**.

All Cargo runs completed sequentially. Extractor check exited **0** at merge
SHA `91d8bd9717`; its inputs remained unchanged through the adapter fix (explicit
diff exit **0**). Original mutation proof remains at `650a576d55`; leaf decoder,
helper and test inputs are byte-unchanged through the fix (diff exit **0**), and
the new native tests passed again. Mutation was not rerun at the fixed SHA.
[GATE_MERGE_f9080e6afb.json](GATE_MERGE_f9080e6afb.json) attaches each result to
its actual measured SHA. Read-only sub-agent review found no adapter blocker.
Movement remains **10 pending->ported, 0->divergent**, census **109/106/316**.


Final conflict resolution/gates: integration advanced to `1181108d2d` and added
its own test-clock adapter. Merge `8c887d8268394e1e8652dbc23abf67008eafc36c`
retains upstream's panic on unexpected monotonic timer use, consistent with
this fixture's sleep/spawn guards. Upstream balance_handle fixture forwarding
is also retained. At that exact merge SHA, scoped all-targets check **0**,
extractor **0** (output read, no manifest drift), new native tests **0 / 5 passed**,
and exchange-state filter **0 / 8 passed**. All Cargo runs were sequential.
[GATE_FINAL_8c887d8268.json](GATE_FINAL_8c887d8268.json) is the final measured
receipt. New leaf source/oracle/test inputs are unchanged from mutation SHA
650a576d55 (diff **0**); mutation was not rerun at this SHA. Sub-agent review
confirmed no port logic was lost. Census remains **109/106/316** and movement
**10 pending->ported, 0->divergent**. Historical gate receipts above remain
attached to their original SHAs, including the measured intermediate failure.
