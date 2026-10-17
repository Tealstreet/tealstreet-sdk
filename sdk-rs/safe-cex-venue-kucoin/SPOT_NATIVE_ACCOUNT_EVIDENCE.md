# Spot native account body components

Zero rows change status: Kucoin remains150 ported /41 divergent /0 pending
across12 manifests/191 rows, with the enumeration stopping at
kucoinspot.ws-public.ts.toml. Four leading class/formatter FINDING rows remain.
The two existing account/validation rows stay divergent: their maintained typed
adapters still have the recorded costs and findings. Adding native components
is progress toward owning-class binding, not a consumer cutover or closure of
those active adapter defects.

`spot_native_accounts` executes the two maintained Spot account arrow bodies
over existing StreamOps language operations. It retains original raw values,
UTF-16 account units, error/reference identity and balance aliases. Request
callee evaluation precedes endpoint lookup; object binding destructures `data`;
catch reporting preserves the source's different evaluation orders in the two
methods. Validation looks up `.catch` before allocating its anonymous zero-arity
handler, calls it with the promise as receiver and awaits its result. A balance
rejection creates an own undefined balance property; a synchronous fetch call
or catch getter failure enters the outer catch instead.

The existing typed account helpers remain maintained and mapped. No shared
Store/error/account ABI changes, runtime shim, alternative transport or local
task registry is introduced. The owning exchange class still uses member
factories, so these two components are not yet installed by that class.
Relevant shared declarations remain in MANIFEST-SPEC.md at
[UTF-8 boundary](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[alias boundary](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias), and
[forced surrogate conflict](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Finite native/source floor

The exact `spot_native_accounts` integration target is one test with20 cases.
Every case finishes decoded native assertions before any source process is
started or any wire/tag projection is computed. Expectations are literal input
cases, field/key lists, counts, numeric bits, UTF-16 units and Rc identities.
No expected count is derived from source output. The source fixture executes
the maintained whole Spot exchange module with explicit Base/import/XHR
fixtures, then calls the actual instance methods. It does not establish full
exchange assembly or transport integration.

The floor stops at case20/pending validation balance settlement:

1. distinct string account fields;
2. numeric/bool/raw-object account identities;
3. falsy userId/uid fallback and null defaults;
4. absent data;
5. raw venue-rejection emission;
6. actual D800/DFFF and mixed surrogate account units;
7. original failure-response message identity;
8. reporter failure inside the try, followed by catch reporting;
9. catch reporter failure escaping unchanged;
10. xhr.get getter changing the endpoint before argument evaluation;
11. four live data getters returning distinct account field values;
12. retained returned balance alias, later free=77;
13. numeric code rejected by strict string comparison;
14. native Infinity rejection-message bits;
15. same raw object returned/emitted, visible marker=97 mutation;
16. balance rejection swallowed with an own undefined property;
17. synchronous balance call failure entering the outer catch;
18. throwing catch getter preceding callback allocation;
19. null response object-binding error;
20. pending balance await, no early result/emission, then original-value settlement.

The JSON capture deliberately tags values, including number bits, undefined,
object key order and UTF-16 units. Native assertions precede that encoding.
The Infinity case proves native value retention; it does not exercise or repair
the legacy AccountValidationError JSON serializer finding. Promise/property
fixtures cover the stated finite operations, not a general engine, sparse or
exotic object semantics, promise-job conformance, cancellation or live XHR.

## Measured code gates

The initial scoped all-target check at
`4393c5f8012eeedb7e31f80afbd8789f206d09b7` exited101 because a scalar-string
fixture helper was unused. It was removed, with no suppression. At
`b9af0cd7f629eb0aaf7efe62acdbb9103333fc22`:

| Gate | Measured exit/result |
| --- | --- |
| Kucoin scoped all-target cargo check, jobs2 | 0;zero errors |
| exact spot_native_accounts target, locked/jobs2/single test thread | 0;one test/20 cases |

Focused KuCoin CI wiring at4393c5f801 exited0 with24 checks. The new target is
mandatory in the package script and has its own command-removal
control. No full venue oracle was run.

## Compiling discrimination controls

At code/test/script baseline
`89f682ecd02d8043c682b8e5d438dba7876ca2d6`, the exact target passed0 before
controls. Each of the following production mutations compiled, failed101 at a
native assertion, was restored byte-for-byte, and then passed the same target0:

| Mutation | Named native assertion |
| --- | --- |
| Swap returned subId/affiliateId | KUCOIN-ACCOUNT native string units |
| Accept numeric200000 alongside the exact string code | KUCOIN-ACCOUNT native rejection shape |
| Await original balance promise instead of its caught promise | KUCOIN-ACCOUNT independent emission count |
| Replace raw catch emission with a diagnostic string | KUCOIN-ACCOUNT native reference identity |
| Omit balance when the caught result is undefined | KUCOIN-ACCOUNT balance property presence |
| Allocate catch handler before reading the live catch property | KUCOIN-ACCOUNT catch handler metadata/allocation order |

For control3, the harness initially expected the later property-presence
assertion, but the earlier independent zero-emission assertion caught the defect
first. The recorded mutant compiled and failed101 at that actual assertion;
restoration passed0 before controls4–6 continued. This was a label correction,
not a survivor or a compiler rejection substituted for a semantic failure.
The control floor stops at iteration6/catch allocation order; no survivor was
observed in these six controls. Source/tests/scripts/package/wiring restoration
against89f682ecd0 was independently measured at exit0.

Read-only sub-agent review found no body mismatch and confirmed native-first
ordering. It required keeping both current rows divergent and warned that the
Infinity packer does not repair the legacy serializer. Both limits are retained.
The reviewer ran no Cargo/tests. Post-metadata/merge gate exits are recorded
separately after measurement.

## Ancestry and exclusions

The branch started at actual integration parent
`333fd419fdbf70f081039610f22da9ee653bc96f`, then adopted locally reviewed,
unmerged feature ancestor14684273d66e798c8a8a00e3968dd15ad96f0c8c and its
Kucoin PR stack. Its ready code was pushed before gates. Do not treat that
ancestor stack as already merged or these measurements as integration gates.

Not run: full venue/workspace/monorepo suites, other venue tests, host/wasm
clippy, wasm builds, general runtime conformance, live transport/account/order
requests or CI reruns. Shared ABI and class-assembly findings remain open.

## Metadata-head measurements

After official parser/serializer reconciliation, the writer exited0 with
updated0/added0/orphaned0. Integration was fetched again and remained at
333fd419fdbf70f081039610f22da9ee653bc96f; merge reported already up to date.
Metadata head`29af8b5a0863f50874ba88dbe816405989e32580` was pushed before:

| Fresh gate at29af8b5a08 | Measured exit/result |
| --- | --- |
| parity:extract:check | 0;would-update0/added0/orphaned0 |
| focused KuCoin wiring | 0;24 checks |
| scoped code/test/script/package/wiring diff against89f682ecd0 | 0 |

No Cargo/tests were rerun at the metadata-only head. Earlier code/compiling
controls remain measurements at their actual SHAs above. Independent tomllib
again measured150/41/0 across12 manifests, stopping atkucoinspot.ws-public.ts.toml;
zero additional rows moved. This receipt-only follow-up changes no gate input.


## Production account-copy cleanup (2026-10-18 SGT)

The unused typed accounts module and its exclusive response_default parser are
retired, with their three account/validation/notification test targets and
source-only oracles. Earlier measurements of those copies are historical. The
constructed Futures and Spot exchanges already use the retained native account
modules; their existing component/source cases remain. The paired package and CI guard now include exchange_trait in the venue oracle command. Changing only the production futures userId read
to referrerUserId compiles and leaves all eight old typed account tests passing,
but fails futures_binds_all_sixteen_defaults at empty ID versus user-1. Exact
restoration passes. The actual constructed test and its runtime fixture are
unchanged, preserving concurrent private-topic work. Canonical shared transport
parsing and all shared body/API/representation policies remain unchanged.
