# Private stream recovery and open sequence

Moved **2 rows pending→ported, 0→divergent**: BitgetPrivateWebsocket.onOpen
and BitgetPrivateWebsocket.recoverMissingPrivateStream.
Python 3.11 tomllib census exited **0** over eleven manifests, independently
expected 227 unchanged identities: **49 ported / 80 divergent / 98 pending →
51 / 80 / 96** against original integration parent
`ef0963be66a6e6f5437788af56e43edf09e0915b`.

Original gate/code SHA: **`d8b0d4f33a29be891c8b3aaf4e0e818899994be3`**.
Its actual immediate parent is ef0963be66a6e6f5437788af56e43edf09e0915b;
no separate unreviewed lane branch was used as an ancestor.
These are the exact field bodies over supplied source receiver/call operations.
Numeric fields retain JS number arithmetic as f64; warning construction preserves
UTF-16 strings including unpaired surrogates. Recovery updates its timestamp
before resolving routing, preserves lazy UTA/current-fallback reads, sets the
fallback flag before logging, and reconnects only after a successful log.
Open calls auth then ping, reads the imported configuration and invokes the
orders method without awaiting it. Full socket assembly, routing resolver,
auth/ping/orders/reconnect implementations remain independently mapped.

## Gates measured at the original code SHA

| Gate | Measured exit / observed output |
| --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --all-targets --jobs 2` | **0**, finished dev profile |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-bitget --test private_recovery --jobs 2 -- --include-ignored` after restoring mutations | **0**, two passed, zero failed/ignored |
| Extractor writer after manifest editing | **0**, 7341 symbols, updated zero manifests, zero added/orphaned |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | **0**, 7341 symbols, would update zero manifests, zero added/orphaned |
| Scoped new-source/test rustfmt check; `git diff --check` | **0**; **0** |
| Restored source/tests diff against original gate SHA | **0** |
| Python3.11 tomllib census | **0**, two pending→ported changes |

Cargo ran sequentially with `/opt/homebrew/opt/rustup/bin` on PATH.
Source oracle env was
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Extractor output text was inspected, not judged solely by exit status.
Manifest mappings/provenance were written with parseManifest/serializeManifest;
seventeen actual receiver helpers have specific exemptions.

## Native assertions and discrimination

Enumeration floor: **403 cases = 388 successful cases + 15 injected failures**.
Recovery success cases enumerate disposed/UTA/current-fallback/resolved-fallback
flags, eight numeric now/last/cooldown triples (boundary, below-boundary, stale,
future-last, NaN, infinity-minus-infinity, NaN cooldown and negative time), and
three UTF-16 reasons (empty, ordinary, unpaired surrogates). Open success cases
enumerate disposed/configuration flags. Failure cases stop at recovery's eleven
operation points and open's four points; the last iteration is open, enabled,
not disposed, failure index 3 (orders method invocation). This is a finite floor,
not completeness of inputs, receiver behavior or the venue.

Every native case asserts operation targets/order, decoded timestamp bits and
fallback state, reconnect/orders counts, decoded warning UTF-16 and Warning enum,
original error identity and UTF-16 payload before output projection. All native
cases finish before any source output comparison. Independent expectations and
literal 403 output count precede comparison. The oracle extracts and executes
the actual maintained TS initializers and actual logging Warning enum.

Two deliberate compiling production mutations each exited **101**, failing
`RECOVERY native operation prefix` in native_private_recovery:

- recoverMissingPrivateStream: cooldown `<` changed to `<=` rejects the boundary.
- onOpen: swap auth and ping rejects the native operation target/order.

No mutation survived. Source bytes were restored; the scoped native/source
suite then exited **0**, two passed.

## Integration merge

Merged integration `9d6f40b96faafcfa73e19ffcfb2825e4d3e78444` down as
**`a03b27bc534d6f171fedeef3ab0314bd6f058b32`**. Merge exit **0**, no conflicts.
Source/tests and source capture scripts diff against original gate SHA exit **0**.
The owned code/assertions are byte-unchanged; original mutation results stand.

At merged SHA a03b27bc534d6f171fedeef3ab0314bd6f058b32:
scoped crate check **0**, scoped private_recovery suite **0** (two passed,
zero failed/ignored), extractor check **0** (7341 symbols, would update zero
manifests, zero added/orphaned), and tomllib census **0** against merged
integration (same two pending→ported changes). The crate dependency source
paths were unchanged; Cargo.lock had one unrelated insertion. Mutations were
not repeated after merging. Evidence-only commits follow this tested SHA.

## Review and non-claims

Native sub-agent `/root/account_read_review` reviewed exact committed
**d8b0d4f33a29be891c8b3aaf4e0e818899994be3**, verified immediate parent
**ef0963be66a6e6f5437788af56e43edf09e0915b**, and reported no blockers;
ported is supportable for the declared typed synchronous bodies. It checked
cooldown/route/read/write/log/reconnect order, auth→ping→config→detached orders
invocation, native-before-wire assertions and scoped helper declarations.
It independently measured actual AST source capture **0** and checker **0**
for independently constructed 403 inputs, 388+15 floor, two ported mappings
and seventeen helper exemptions. Reviewer did not run Cargo, extractor,
mutations, clippy/wasm or CI; primary measured those gates reported above.

Not run by primary: full venue tests, workspace compilation, monorepo suites,
host clippy, wasm clippy/check, CI, invention checker, network exchange tests
or concrete private-socket/routing binding. Runtime values outside the declared
bool/number/string contract, concrete scheduler/socket assembly and called
async implementation/rejection handling are unproven, independently mapped
work. No full venue completion claim is made.
