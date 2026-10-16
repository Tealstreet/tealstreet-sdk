# OKX reference-price parity

Four rows moved pending -> ported; zero divergent rows added. Together with
KuCoin's three new subscription rows this slice moves seven pending rows.
Independent Python 3.11 tomllib census visited all 12 OKX manifests and stopped
at `okxeudemo.exchange.ts.toml`: parent 55 ported / 19 divergent / 89 pending;
delivery code head 59 / 19 / 85. Census and behavioral proof are separate claims.

Fresh branch `feat/okx-kucoin-market-reads` was based directly on integration
`c54039795784efc67fbfa0bb7d0109e47440ee11`; no old feature branch was inherited.
Source/control SHA: `354cf116b3c1d6eae2a3d9a88df50272102ebdaf`.
Lookup-order/plumbing fix: `3823d4807c3c0f49a706af67bc5f872d3dc32267`.
First merge incorporated integration `ebcb51ffa55881e2e5a745c0d419028b0632da7d`
at measured cargo-gate SHA `4869a732862b7f2bda8a9462a6cc8bc0286d152d`.
Exact rename-note correction: `1a35e412cb415caee34fba1587aa2c5d1d7558a2`.
Pre-push merge incorporated integration `d4d9ae0b019e4c5466a67294aa474f897ee38061`
at metadata-gate SHA `1d0e7bd41a3f6b05b5210fb7980f41278822b9ed`.
There is no unmerged foreign feature branch among these merge parents.

## Mapped bodies and host contracts

`OKXExchange#getTicker`, `#resolveReferencePrice`,
`#hasExternalTriggerReferencePrice`, and `#resolveTriggerReferencePrice` map to
their snake-case functions in `src/reference_price.rs`. Ticker lookup preserves
lazy ID-first selection and opaque Rc identity. Each host callback evaluates
the complete table/key/entry lookup expression, including table acquisition
before key reads and inherited-member semantics. Concrete host wiring remains
pending with the exchange class.

ReferenceTicker is a live numeric-property read capability, preserving getters
that mutate sibling fields or throw. Declared numeric fields retain NaN,
infinities, signed zero, and missing values. Numeric conversion retains the
source String(number) -> parseFloat -> finite sequence. The unknown-input
`toFiniteNumber` row remains pending; this numeric adapter does not port that
broader object/string-coercion function. Loaded predicates read current market
ID/set state after the initial ticker lookup. The in-process host callback owns
optional method lookup/call and lazy ID reads. Trigger strings use UTF16 units.

Five exact C3 plumbing exemptions identify the new field/source/config selectors,
ReferenceTicker trait, and its read method. None exempts a venue algorithm.
All seven new row notes name exact adapted targets. Six already-ported rows from
my previous state/disposal slice also received required exact rename declarations;
their status and behavior did not change and they add zero rows to this count.

## Measured gates

Oracle environment:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
Cargo invocations were sequential, with two jobs each.

| Command | Measured SHA | Exit |
| --- | --- | --- |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --all-targets --jobs 2` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-okx --jobs 2 --test reference_price` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 0 |
| Same exact extractor check | `1d0e7bd41a3f6b05b5210fb7980f41278822b9ed` | 0 |
| `sdk-rs/target/debug/parity-check --workspace sdk-rs` | `4869a732862b7f2bda8a9462a6cc8bc0286d152d` | 1 |
| Same exact existing-binary Rust-index check | `1d0e7bd41a3f6b05b5210fb7980f41278822b9ed` | 1 |

Both extractor outputs reported 7341 symbols / 512 source files and
`Would update 0 manifests; 0 added, 0 orphaned symbols`. Writer runs after
manifest edits also reported zero updates/additions/orphans. Arrays and new
exemptions were written with the TOML serializers.

The auxiliary existing Rust-index binary was not rebuilt. Both runs stopped
before status/invention validation on `safe-cex-webworker/src/twap_manager.rs`:
`item macro thread_local cannot be indexed without expansion`. A separate read
of the current indexer source confirmed that same macro rejection. This is an
unresolved metadata-instrument limitation; no passing full Rust-index result
or validated global invention count is claimed.

The four Rust method bodies have 96 declared native/source cases: 24 lookup,
12 reference, 42 external, 18 trigger cases. Every native value, call order,
pointer identity and complete error assertion runs before the live oracle.
Expected numeric values and trace order are independent fixtures; comparisons
use actual native f64 bits, not re-encoded objects. Additional paired witnesses
exercise a callback changing the later symbol key, absent ticker behavior, and
initial lookup changing the market ID before the loaded predicate.

Enumeration floors stopped at: lookup surrogate key / both hits / ID error;
reference missing values / success; external throwing callback / in-process /
index; trigger last-price fallback / second-lookup error. These finite fixtures
do not claim exhaustive input enumeration or concrete exchange lifecycle proof.

## Discrimination and preservation

At `354cf116b3c1d6eae2a3d9a88df50272102ebdaf`, each of these compiling controls
exited 101 at the named native assertion, then its byte-restored identical focused
test command exited 0:

| Production mutation | Failed native assertion |
| --- | --- |
| Drop an ID ticker hit | `native lookup presence` |
| Raise reference acceptance threshold from 0 to 10 | `OKX-REFERENCE native reference value` |
| Change external unequal-price test to equality | `OKX-REFERENCE native external availability` |
| Read mark instead of index in trigger resolution | `OKX-REFERENCE native trigger value` |

After the lookup API review fix, dropping an ID hit was repeated at
`3823d4807c3c0f49a706af67bc5f872d3dc32267`: compiling failure 101 at
`native lookup presence`, restored rerun 0. The other three function bodies
compared byte-identically from their first declaration through EOF between the
two source SHAs; that comparison returned true. No control result is relabeled
as a measurement of a newer SHA. Zero survivors among these five controls.

First merge's exact whole-venue src/tests/scripts/manifests comparison against
`3823d4807c` exited 0. Final merge's exact src/tests/scripts comparison against
`4869a73286` also exited 0. The checked shared Rust dependency sources and Cargo
manifests/lockfile had zero changed paths; live OKX/KuCoin TS oracle inputs and
source_loader comparison exited 0. Hence cargo measurements remain attached to
`4869a73286`; only changed metadata gates were refreshed at `1d0e7bd41a`.
Receipt commit `5510394b7d` differed from `1d0e7bd41a` only in these two receipts.

Sub-agent reviews found getter-ordering and plumbing-exemption issues, then
verified those fixes and the exact rename declarations and reported no remaining
static blocker. Reviewer ran no gates. Not run: workspace compilation, full venue suites,
host clippy, wasm clippy, CI, network requests, or concrete exchange wiring.

## Final integration merge

The final fetch found integration `9f59912c6f39a0df543e6228c38ad5a57258d2a2`.
Merge SHA `10fddfe345549783937235fb8566a72524d055e6` resolved one append conflict
in exempt.toml by retaining our five entries and all 22 incoming Bitget entries.
The serializer-based merge compared every row and reason against the exact union
of both stage inputs and rejected duplicates. Sub-agent parent-diff review
confirmed both sets survived without deletions and found no resolution issue.

At `10fddfe345549783937235fb8566a72524d055e6`, the exact extractor command above
exited 0: 7341 symbols / 512 files, zero updates/additions/orphans. Exact
`git diff --exit-code 4869a73286 HEAD --` over both venue src/tests/scripts exited
0. Checked Cargo manifests/lockfile, shared Rust dependency sources, live venue
TS inputs, and source_loader had zero changed paths. The four cargo exits remain
measurements of `4869a73286`, not newly measured exits of this merge SHA.
The auxiliary Rust-index check was not repeated after this last merge and remains
unverified; its two previously measured failures remain recorded above.

The final independent tomllib comparison at this SHA found exactly four OKX and
three KuCoin pending -> ported changes, and no other status changes, relative to
`c540397957`. Counts remain OKX 59 / 19 / 85 and KuCoin 76 / 39 / 76, after all
12 manifests per venue. Delivery after this merge changes only these two receipts.
