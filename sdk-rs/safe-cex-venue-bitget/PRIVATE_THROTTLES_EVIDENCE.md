# Private fetch throttle bodies

Moved **3 rows pending→ported, 0→divergent**: throttledFetchPositions,
throttledFetchBalance and throttledFetchOrders. Python3.11 tomllib census exit
**0** over eleven Bitget manifests and independently expected 227 unchanged
identities: **51 ported / 80 divergent / 96 pending → 54 / 80 / 93**.
Measured against both original integration parent
`4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a` and merged integration
`d8ab034625643795919e84c63ee6e5d764cb11df`. Census is not full venue proof.

Tested implementation SHA: **`830126ccdd34be377420b2bd31b588d4705a5a0e`**.
Its actual immediate parent is 4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a;
no separate unreviewed lane work is an ancestor. These exact typed field bodies
and inline callbacks use supplied fallible source receiver/call sites. They keep
Date.now before disposed, the balance-only pending write before scheduled,
separate throttle reads, raw f64 delay, live callback guards and detached fetch
invocation. Concrete scheduler/private socket/execute*Fetch implementations
remain independently mapped. No full assembly claim is made.

## Gates at the tested implementation SHA

| Gate | Measured exit / observed output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, finished dev profile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test private_throttles --jobs 2 -- --include-ignored` after restoring mutations | **0**, three passed, zero failed/ignored |
| Extractor writer after manifest editing | **0**, 7341 symbols; updated zero manifests, zero added/orphaned |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | **0**, 7341 symbols; would update zero manifests, zero added/orphaned |
| Scoped new-production/test rustfmt check; `git diff --check` | **0**; **0** |
| Restored source/tests diff against tested SHA | **0** |
| Python3.11 tomllib census | **0**, three pending→ported changes |

Cargo ran sequentially with `/opt/homebrew/opt/rustup/bin` on PATH.
Source oracle used
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output text was inspected. Mappings/provenance were written with
parseManifest/serializeManifest; fourteen actual helpers have specific exemptions.

## Native floor and deliberate breaks

Independent floor: **190 cases = 153 successful combinations + 37 injected
operation failures**, plus a separate receiver-retention test. It stops at Orders,
below boundary, pending=true, deferred callback, not disposed, failure index 11
(callback launch). Success cases cover three families, disposed/scheduled/pending
flags, boundary/below/NaN timing, callback disposed flags, and one inline,
new-pending-at-fire and changed-second-throttle case per family. This is a floor,
not exhaustive numeric/runtime input or concrete scheduler coverage.

All native cases assert exact enum-selected field/fetch targets, operation
prefixes, original setup/callback error identity and UTF-16 payload, independent
read/timer counts, scheduled/pending states, launch counts, callback invocation,
and raw delay f64 bits/NaN before output projection. All cases finish native
assertions before source comparisons. A retention test drops the caller, runs
the retained receiver's callback, then proves release after dropping it.
The source oracle extracts the actual maintained three TS initializers and
independently verifies source throttle defaults [500, 300, 500]. Native and
source 190-case counts are independently fixed expectations.

Each following production mutation compiled and exited **101**, failing
`THROTTLE native decoded raw delay` in native_private_throttles:

- Positions: replace second-throttle minus elapsed with plus elapsed.
- Balance: replace second-throttle minus elapsed with plus elapsed.
- Orders: replace second-throttle minus elapsed with plus elapsed.

No mutation survived. Source was restored byte-for-byte; restored native/source/
retention suite exit **0**, three passed. Before the implementation commit, an
initial uncommitted test run exited **101** on JSON 1.0-versus-1 representation;
native cases passed. The wire projection was corrected after native assertions,
and subsequent native/source checks passed. That initial run is not a gate for
the tested SHA.

## Integration merge and review

Merged d8ab034625643795919e84c63ee6e5d764cb11df down as
**`a2f0a949ee648f3708b55fbc42b85655d9c840eb`**. Initial merge stopped with an
exempt.toml append conflict; resolution preserved every integration exemption
and the fourteen exact own helper records, with zero duplicates (Python check
exit **0**). Merge commit succeeded. Source/tests/capture scripts scoped diff
against tested implementation SHA exit **0**. Bitget dependency source/Cargo.lock
paths were unchanged in this merge, so original Cargo and mutation gates stand;
those gates were not rerun. Post-merge census exit **0**, same three changes.

Native sub-agent `/root/account_read_review` reviewed exact committed
830126ccdd34be377420b2bd31b588d4705a5a0e and verified its actual parent. No blockers;
ported classification is reasonable within typed supplied-operation scope.
Reviewer measured actual AST source capture **0** with independently generated
190 inputs and Python3.11 checker **0** for 153+37 cases, negative/NaN delay
witnesses, prefix/disposition checks, three mappings and fourteen exemptions.
Reviewer did not run Cargo, extractor, mutations, clippy/wasm or CI; gate results
above belong to the primary agent.

Not run by primary: full venue tests, workspace compilation, monorepo suites,
host clippy, wasm clippy/check, CI, invention checker, network exchange tests or
concrete scheduler/socket binding. Typed bool/number input scope, non-exhaustive
numeric cases and independently pending fetch implementations are explicit limits.

At merge SHA a2f0a949ee648f3708b55fbc42b85655d9c840eb, extractor check
exit **0**: 7341 symbols, would update zero manifests, zero added/orphaned.
Evidence-only commits follow the tested code/merge SHAs.

Final fetch merged integration 976c7bf08eb0d23329d7dfd9042035e3aabe2358 as
3597c79c5dea4e340c155266a35144d8b9ede1df without conflicts. Only an unrelated
TWAP manifest changed since d8ab034625. Own source/tests/capture scripts diff
against 830126ccdd exit **0**; Bitget/dependency source/Cargo.lock paths unchanged.
Extractor rerun at 3597c79c5dea4e340c155266a35144d8b9ede1df exit **0**:
7341 symbols, would update zero manifests, zero added/orphaned. Original Cargo/
mutation gates stand. This final note changes evidence only.
