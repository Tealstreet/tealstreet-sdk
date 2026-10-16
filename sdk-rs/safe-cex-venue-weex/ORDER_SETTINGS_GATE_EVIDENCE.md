# WEEX order-result and symbol-settings gates

Authoring parent: `11bafd67b4950c2120105f37929798edd9c9c032` on
`feat/safe-cex-rust`. Fresh slice branch: `feat/rust-venue-weex-order-results-settings`.
No unrelated unreviewed feature branch was used as an ancestor.

## Rows and implementation scope

Independent tomllib comparison against that parent moved **3 pending -> ported,
0 pending -> divergent**: `WeexExchange#parseOrderResult`,
`WeexExchange#getSymbolConfigFromCache`, `WeexExchange#getOrFetchSymbolConfig`.
WEEX census: 65 ported / 26 divergent / 65 pending, 156 rows.
The canonical scoped gate validates the six new actual Rust targets and all
WEEX off-pending mappings; 112 public Rust items, zero WEEX exemptions.

Order-result parsing retains optional access, strict false rejection, lazy
errorMessage/errorCode and orderId/clientOrderId/fallback/uuid selection,
emit-before-Error and imported toIntegerString conversion. Opaque native values
and fallible primitive capabilities preserve the algorithm without restricting
inputs to JSON. The oracle extracts actual source fields and toIntegerString,
and loads the actual shared enum module; it does not copy their implementations.

Symbol lookup retains cache identity by returning the selected opaque value,
refetch short circuit, eager cache/fetch invocation, awaited fetch, subsequent
map lookup and all five default fields. The account mode is read after await.
Shared host references avoid an exclusive host borrow across the fetch; the
fixture uses scoped RefCell borrows and accesses the host before first poll.
Class assembly, REST fetch implementation, host value primitives and executor /
JavaScript microtask scheduling remain outside this function-algorithm claim.
Object-reference identity is preserved by the generic return path but was not
asserted by these scalar cache/fetched fixtures.

## Measurements at implementation SHA

`99cd907e62869c55aa68c7ef5b74b90eb0967a81`:

| Command | Measured exit / result |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2` | 0; scoped compilation, zero errors |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/order-settings-mutations.py` | 0; three compiled mutations caught, each restored target passed |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 1; two new Rust files missing from file.rust_files; fixed in the manifest-only commit below |

Each mutation runs the specific target:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test order_settings`.
Each broken run exited **101** with assertion left==right failure text and the
named native assertion; each restored run exited **0**, one test passed.

| Source row / selected production mutation | Named assertion | Broken / restored exit |
| --- | --- | --- |
| parseOrderResult: choose clientOrderId in place of orderId | `native_parse_output::"order_id"` | 101 / 0 |
| getSymbolConfigFromCache: pass an empty symbol to cache get | `native_settings_symbols::"cache_hit"` | 101 / 0 |
| getOrFetchSymbolConfig: change default cross leverage from 20 to 21 | `native_settings_output::"default_after_await"` | 101 / 0 |

Enumeration stopped after these three controls: three caught, zero survivors
among those controls. Other mutations were not enumerated. Source bytes were
restored after each run; compiler errors are not accepted as discrimination.

The native target stops at the ordered 20 order-result and 12 symbol-lookup
cases, 32 total. IDs are fixed independently in Rust; this is a finite floor.
Native UTF16 outputs, error names/messages, emitted values, symbol arguments,
five default fields, eager-start effects and two fetch polls are asserted before
comparison of complete effect traces. No decode/re-encode comparison is used.
Cases include lazy fallbacks, falsy values, source rejection, surrogate strings,
getter/coercion/emitter failures, cache hits, forced refetch, suspension and
post-await account-mode defaults. This does not prove every JavaScript input.

## Measurements at manifest correction SHA

`bc873a317366caa2833730fd71388192a3cee761` changes only file.rust_files.
`git diff --exit-code 99cd907e62869c55aa68c7ef5b74b90eb0967a81 HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests`
exited 0 here; implementation/test measurements remain bound to the earlier SHA.

| Command | Measured exit / result |
| --- | --- |
| `python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py` | 0; 156 rows, 112 public items, zero WEEX exemptions |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0; 7341 symbols; Would update 0 manifests, 0 added, 0 orphaned |

The extractor writer was run after manifest edits. The check's output text was
read; these claims do not depend solely on a Volta exit status.

Sub-agent weex_review inspected actual source, native implementation, fixtures,
three mappings and controls. It found the initial lazy async invocation boundary;
the eager-start fix above addresses it. Follow-up inspection found no concrete
remaining local algorithm defect. It ran no cargo/tests/mutations and made no
edits or commits; its inspection is separate from the measured gates.

Not run: workspace compilation, full venue/monorepo suites, host clippy, wasm
clippy, live venue requests, or CI reruns. No income pager was added.
Ignored local logs and control results: `sdk-rs/target/weex-order-settings/`.

## Delivery merge

Fetched integration as this worktree's exact FETCH_HEAD
`cd26f67d81a2b5af705138b5cfafa40b0f9a44a2` and merged it without conflicts into
`bb039d8913bd874ae3fa3f9971f46ec2db5ec6f7`.
A tomllib audit against that actual integration parent exited 0 and required
exactly the same three pending -> ported transitions, zero divergent transitions,
156 total rows and no missing Rust target files. Census remains 65 / 26 / 65.

`git diff --exit-code 99cd907e62869c55aa68c7ef5b74b90eb0967a81 HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests`
exited 0 at the merge. A separate git diff from the manifest correction SHA
exited 0 for WEEX source/tests/Cargo.toml/manifests, workspace Cargo manifest /
lock, shared types/num/store, canonical parity engine/extractor/exemptions,
package.json, actual WEEX TypeScript/shared enums/logging enum and Cargo
configuration paths. The new evidence document is excluded from that input check.
Original compilation, native and mutation measurements remain bound to their
original implementation SHA; the scoped parity measurement remains bound to
its manifest correction SHA. They were not rerun at the merge because their
inputs are byte-unchanged.

Global manifests changed in integration, so
`node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`
was remeasured at `bb039d8913bd874ae3fa3f9971f46ec2db5ec6f7`: exit 0;
7341 symbols, Would update 0 manifests, 0 added, 0 orphaned.
This evidence update changes only this narrative.

The initial local orchestration wrapper exited 1 after the successful extractor
at the manifest correction SHA: it expected writer wording "Updated" instead of
check wording "Would update". The actual extractor command exited 0 and its
zero-drift output was read directly; no wrapper-pass claim is made.
