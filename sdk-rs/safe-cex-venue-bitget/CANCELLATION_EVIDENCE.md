# Cancellation family

Moved **9 rows pending→ported, 0→divergent**. Python3.11 tomllib census exit
**0** across eleven Bitget manifests, independently expected 227 unchanged
identities: **54 ported / 80 divergent / 93 pending → 63 / 80 / 84** against
integration parent **775861bde0e6918da558dce50cd5892c6087ae61**.

Rows: BitgetExchange cancelAllOrders, cancelOrders, cancelNormalOrders,
cancelUtaNormalOrders, cancelAlgoOrders, cancelSymbolOrders; BitgetInverseExchange
cancelAllOrders, cancelNormalOrders, cancelAlgoOrders. UTA strategy cancellation's
not-found policy remains pending; its delegate capability is not its implementation.

Final tested/reviewed implementation SHA:
**c97efcd5c4bcda4edf0c7e7a68c0951e9228c5c9**.
The branch was created directly from the integration parent above. c97 follows
own implementation commit 3d9e77a9dcd1446a477da0683b29a6ffda378c7b and fixes two
review findings in that commit. No separate unreviewed lane branch is an ancestor.
The first commit is not claimed as the finished implementation.

## Measured gates at c97efcd5c4

| Gate | Exit / observed result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, finished dev profile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test cancellation --jobs 2 -- --include-ignored` after restoring controls | **0**, two passed, zero failed/ignored |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --jobs 2 -- --include-ignored` | **0**, all Bitget unit/integration binaries passed, no ignored tests; doc-test phase passed with zero tests |
| Extractor writer after manifest editing | **0**, 7341 symbols; updated zero manifests, zero added/orphaned |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | **0**, 7341 symbols; would update zero manifests, zero added/orphaned |
| Scoped production/test rustfmt check; `git diff --check` | **0**; **0** |
| Restored source/tests diff against tested implementation SHA | **0** |
| Python3.11 tomllib census and 49-helper uniqueness checks | **0** |

Cargo was sequential; PATH included `/opt/homebrew/opt/rustup/bin`.
Oracle env: SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript.
Extractor output text was inspected. Mappings/provenance used
parseManifest/serializeManifest; actual helper entries used smol-toml serialization.

## Native proof and finite floor

Independent floor **88 scenarios = 67 ordinary/live success scenarios + 20
injected-error scenarios + one inherited-key failure**. It stops at partition,
small input, UTA=false, predicate truncation: the next missing element is passed
to the source predicate as undefined, which throws the original injected error.
This is a floor, not exhaustive data/getter/scheduling or full venue proof.

Native assertions precede every projection; all native fixtures finish before
source comparisons. They inspect actual UTF-16 IDs/symbols/scalars, field presence
and property order, endpoint, decoded batch IDs, opaque info and original record
alias identities, request/delegate/error/write-check counts, operation order,
original error identity/payload and error disposition. Counts and expected batch/
partition contents come from fixed input scenarios and independent literal
expectations, not from emitted requests. The oracle executes the nine actual TS
initializers with actual Lodash, omitUndefined and source ENDPOINTS.

Dense caller arrays remain shared and mutable. Witnesses cover appending during
account detection, appending between algo requests, replacing a later entry from
a predicate or symbol getter, and truncating during partition. Array.filter skips
missing entries; Lodash partition invokes the predicate with undefined. Numeric
symbol keys are ordered as Object.entries; classic deduplication is global,
inverse deduplication is per symbol. Normalization preserves original records and
info aliases; UTA omits undefined without discarding valid opaque values.

One compiling control per cancellation family, each at c97efcd5c4:

| Family / mutation | Exit and named assertion |
| --- | --- |
| Classic: batch size 50→49 | **101**, `CANCEL native decoded batch IDs` |
| UTA: bypass undefined omission while retaining a valid mutable payload | **101**, `CANCEL native own payload field count` |
| Inverse: rethrow a per-order error that the source swallows | **101**, `CANCEL native error disposition` |

No final control survived. Source bytes were restored; cancellation suite **0**,
then full Bitget suite **0**. An earlier UTA control removed the retain operation
and failed compilation under unused-mut denial; it was not a discriminating gate.
The corrected control compiled and failed the named assertion above. Intermediate
uncommitted test compilation diagnostics were fixed before these gates.

## Review citation and limits

Native sub-agent /root/account_read_review reviewed exact c97efcd5c4 and cleared
both P2 findings at the earlier 3d9 commit. Those findings were whole-array
snapshots during partition and symbol filtering; final loops capture initial
length and read each index live, with borrows released before supplied callbacks.
Final review found no further blockers and supported the nine body mappings.
Reviewer measured committed source-script equality, actual AST source capture
**0** on independently generated 88 inputs and checker **0** for replacement/
filter/truncation witnesses, 9 mappings and 49 actual helper targets.
Reviewer did not run Cargo, extractor, mutations, clippy, wasm or CI; gate numbers
above were measured by the primary agent.

Source property/spread/ordinary-object operations and Promise.all are supplied
language capabilities. Promise.all first temporal rejection and sibling work
survival are contractual, not proven by the already-settled fixture promises.
Polling is the Rust async execution boundary. Sparse/proxy arrays, exhaustive
getter/race behavior, concrete runtime/REST/exchange binding and called child
implementations remain unestablished. No full assembly or entire venue claim.

Not run by primary: workspace compilation, monorepo suites, host clippy,
wasm clippy/check, CI, invention checker or network exchange tests.

## Integration preservation

Merged integration **200ef7e5dd8ead4ef9998682f302d272ca8a0dae** down as
**16acf0c05402bcbb37ce87abce5f657f33a38c8d**. Initial merge stopped on an
exempt.toml append conflict. Resolution retained the integration file verbatim
plus all 49 exact own helper entries, using smol-toml serialization. Python
verification exited **0**: every integration entry and own helper preserved,
zero duplicate rs targets. No CLAUDE conflict occurred.

Scoped source/tests/capture-script diff against c97efcd5c4 exited **0**.
Owned implementation/assertions are byte-unchanged. Shared transport source and
Cargo.lock changed, so gates whose relevant inputs changed were rerun:

- Scoped crate check at 16acf0c054: **0**, finished dev profile.
- Cancellation target at 16acf0c054, including oracle: **0**, two passed.
- Extractor check at 16acf0c054: **0**, 7341 symbols, would update zero manifests,
  zero added/orphaned.
- Tomllib census against merged integration: **0**, same nine pending→ported
  changes and 63/80/84 triple.

The full Bitget suite and compiling controls were **not** rerun after the merge;
their measured exits belong to c97efcd5c4, whose own source/tests are unchanged.
Evidence-only commits follow the measured code/merge SHAs.

Final fetch merged integration **5eabdd1396505c2cb959c7870441d20492738663** as
**0ee33e5f3bec6aed21092d8e182b6336ccde184f**, without conflicts. Only two
unrelated Binance manifests changed since 200ef7e5dd. Own source/tests/scripts
scoped diff against c97efcd5c4 exit **0**; Bitget/dependency source/Cargo.lock
paths were unchanged since the measured post-merge gates. Final extractor check
at 0ee33e5f3b exit **0**, 7341 symbols, would update zero manifests, zero added/
orphaned. The measured Cargo/control exits above stand at their stated SHAs;
this final note changes evidence only.
