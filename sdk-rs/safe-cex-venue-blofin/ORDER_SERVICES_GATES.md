# Blofin order-formatting and REST service slice

Moved **9 pending rows to divergent; 0 to ported**. Starting parent:
**661022ccc76613805ba357730a6a30475608c4ea**, the reviewed PR7522 integration head.
No unrelated unreviewed branch is an ancestor. Initial merge-down integration
parent: **976c7bf08eb0d23329d7dfd9042035e3aabe2358**.
Python3.11 `tomllib` census at exact code SHA
**2c46a8e26fc846bd9789d26df9309e4cc92175e3**: before14/31/80,
after **14 ported /40 divergent /71 pending**,125 rows across7 manifests.
Stopped at manifest7;9 independent previously pending method names are attached.
Census is not exhaustive behavioral proof.

Implemented formatTriggerOrder, formatNormalOrder, formatAlgoOrder,
getOrFetchPositionMode, getOrFetchMarginType, fetchLeverageForSymbol,
changePositionMode, placeTriggerOrder and cancelSymbolOrders. All rs targets are
real executable methods or their concrete host/error/payload carriers. Class,
API construction/auth, cancelOrders saga and websocket clients remain pending.
Specific CHOSEN notes cover typed callbacks, owned snapshots versus JS aliases,
closed model/JSON/error profiles, host-error Display and10000 total split payloads.
No market fields were changed.

Measured at **2c46a8e26fc846bd9789d26df9309e4cc92175e3**:

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-blofin
  --all-targets --jobs 2`: **0**.
- `cargo clippy`, same manifest/package/all-targets/jobs, `-- -D warnings`:
  **0**, host only.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript
  cargo test`, same manifest/package, `--test order_services --jobs 2
  -- --nocapture`: **0**, **10 tests**.
- `rustfmt --edition 2024 --check` on new formatter/service source and test:
  **0**. This is changed-file formatting, not workspace formatting.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`:
  **0**; output7341 symbols/512 files and **would update0** manifests. The writer
  previously canonicalized1 changed manifest after rs arrays were generated.
- Canonical source coverage `tests/coverage_scope.cjs`: **0**; source/manifest
  declarations agree at independent floors20 types /3 income /3 history /66
  exchange, stopping after4 files and92 declarations.
- Preexisting local `sdk-rs/target/debug/parity-check`, Blofin-only indexed
  workspace/all7 venue manifests/empty exemption file: **1**. Only inherited
  `position_adapter.rs::{BlofinPositionAdapterDeps,map_blofin_position_history,num}`
  lack manifest/exemption entries. Scoped `git diff --exit-code 661022ccc7 HEAD --`
  that file measured **0**. No exemptions/targets were added to conceal these;
  no current checker rebuild or global Rust-parity pass is claimed.

The oracle runs exact maintained method AST initializers with genuine imported
numeric functions, enums, constants and actual TS DefaultStore. Its independent
floors stop after **29 formatter cases +34 service cases =63**; each service case
runs native encoded/untransformed/transformed response paths (**102 native service
comparisons**). Native maps are inspected property-by-property; buy/sell leverage,
caches, position fields, callback order/error origin and forwarded native Order
fields are checked before request/diagnostic records. No decode/reencode equality
is used. Source fixed anchors independently pin amount12, orderPrice101.5,
TP149.5/SL73.25, split counts5/3/0 and both10001-allocation witnesses before JSON.
Caller-ID policy is an explicit dependency hook; this does not prove pending
full-class policy wiring. Futures are ready fixtures; delayed scheduling remains
unproven. Opaque executable/prototype objects and JS exception classes remain
outside the declared typed adapter domain.

At exact code SHA, **12 serial production controls** caught all named runtime
assertions at **101**, then passed byte-restored at **0**, stopping at12/12.
Nine controls cover the newly mapped rows; three additional controls cover concrete
review/edge regressions. No survivor or compiler failure was counted as proof.
Per-control call sites, assertion names, exits and raw logs are committed under
`order-services-evidence/controls/`. Earlier development compiler, fixture-required-
field and collapsible-if lint failures were corrected before this final SHA; their
partial passes are not relabeled as final gates.

Sub-agent ticker_review static review bound to exact
**2c46a8e26fc846bd9789d26df9309e4cc92175e3**: no remaining findings. Confirmed P2
findings were fixed: remainder now counts toward total allocation before split
UUID generation, and invalid optional failed-response hints fall back to original
transport messages. Source/native witnesses cover9999.5/max1/lot0.5 producing10001
source payloads, and HTML hints across3 adapter representations. A later strict
rows.length===0 short circuit now handles a JSON object{length:0} quietly like TS.
The logger-borrow candidate was withdrawn after examining format!'s temporary
scope; no logger-borrow fix is claimed. Reviewer ran no Cargo/oracle/controls.

Full venue test suites, full workspace builds/tests, wasm check/clippy, live
network/venue calls, API signing construction, websocket runtime tests, CI reruns
and a new global inherited Rust-parity enumeration were **not run**. Final merge
preservation is recorded separately; gates remain attached to the code SHA.
