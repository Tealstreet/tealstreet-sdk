# Live Coinbase class policy and retained Promise values

This repair removes copied class policy from portfolio selection and preserves
separate source arrow-field bodies. It does **not** close the Exchange or Spot
class FINDING: API/socket parent dispatch and the composition of startup, tick,
Store and other local methods still need a concrete runtime join. The fourteen existing
member-level notification/error-carrier FINDINGs also remain independently open,
alongside the newly withdrawn snapshot publication row below.
The mapping review also exposed the class applyOrderSnapshotBatch publication carrier gap. That previously ported row is withdrawn to divergent/FINDING; the typed standalone Store helper remains mapped. This withdrawal is a newly discovered finding, separate from the later explanation-only divergence audit.

The class reads its current options slot and supportSpot value at the source
call sites, including after a pending selection resumes. Spot replaces the two
portfolio fields after inherited construction. A retained base arrow keeps its
original body and bound receiver; assigning it to another instance preserves the
original receiver, including live options and HTTP leaves after external root
handles are dropped. Private self-relative storage avoids the ordinary strong
self cycle, while every outward field read returns a strong bound value; when that body reads another field, it sees
the receiver's current field. The same Promise returns the same mutable native
supported-types array or account object on repeated awaits. A fresh invocation
returns a fresh root. The earlier method implementations copied those values;
that was a real defect, not a language restriction. See
[the alias declaration](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias).

The shared signing getter now reads lazy operands in source order and preserves
the virtual canSign callback used by isPublicDataOnly. Native trace tests cover
five fixed operand paths plus the explicit public-data short circuit and virtual
callback results. A class fixture uses real constructed roots and the actual
TypeScript class definitions/getters; its Base constructor, HTTP and Store
operations remain supplied leaves. Its fourteen literal policy scenarios stop
at live-policy-after-pending-selection. A separate four-case arrow-value floor
stops at reassigned-arrow-keeps-old-code-and-origin; neither floor is an
exhaustive enumeration. This is a floor, not a whole-class or
all-input parity claim.

## Measured revisions

All evidence files live in `evidence/live-class-policy/` and identify the source
revision they actually measured. The pre-arrow baseline gate source is
`5ce7122d895f27483e7de0b7228a75d2a6e0afca`. It includes the shared exchange helper,
live class policy and retained Promise-root changes. It does not include the
subsequent cross-instance arrow assignment repair or its four additional tests. Gates were measured
before later documentation/manifests or integration merge-downs; those later
commits are not silently covered by an earlier result.

The scoped check and five affected test targets passed at that revision. The
five affected targets ran eighteen Rust tests, including the fourteen-scenario
class policy test. The full venue runner at that baseline passed 259 Rust tests
in 53 result blocks, thirteen declaration tests, and all 262 audited reachability
roots (248 ported roots). Scoped host and wasm clippy each exited zero. The
extractor reported zero updates, additions or orphaned symbols. Native assertions run before any wire assertions and retain the
actual result roots. The HTTP expectations pin endpoint and UUID content, not
just the number of requests.

Six production mutations each failed Cargo with exit 101 at the named native
assertion and passed with exit 0 after restoration. `baseline-controls.json` pins the six
literal IDs, call sites, assertion names, commands and original/mutant/restored
hashes. Enumeration stopped after captured-base-current-finder. There were no
survivors among these six controls; no exhaustive mutation claim is made.
The portable replay script is `scripts/class-policy-controls.py`; its caller
supplies the oracle environment and output directory. The recorded run used
the same six mutations from the development driver; the portable wrapper itself
is not being represented as an independently rerun mutation gate.

`prior-promise-values.json` records two desired-behavior tests applied to the
actual earlier mapped implementations at
`87ba2a88b407164ca5685785205f18cfa11f08eb`. Both failed at named native assertions:
repeated awaits lost the caller's array or account-object mutation. Test-only
supplements were removed and the original targets passed after restoration.
The current class tests exercise those retained values on actual current class
roots, rather than decode/re-encode roundtrips.

## Attempts and limits

A check at `bad4c8dd9960882b8cedd12fa4e5bd9f39bc5acb` exited 101 on missing
incremental-cache artifacts in a dependency. A later test attempt at the baseline
source revision was interrupted with SIGINT (Python return -2) while the macOS
loader stalled on a dependency procedural macro. Neither counts as a pass.
After a scoped clean of that dependency, the fresh scoped check and all five
affected test targets passed; `gate-attempts.json` retains the failed and
interrupted attempts. A successful retry with an uncommitted formatting change
is deliberately not claimed as a clean-revision gate. The first host-clippy
attempt at the baseline revision exited 255 while the macOS loader stalled on
yoke_derive; a scoped dependency clean and rebuild preceded the passing host
and separate wasm runs. That interrupted attempt remains a measured failure,
not a pass.

Broad workspace check/build and the full host/wasm matrix were measured on an
earlier integration snapshot in PR 7928. Those results do not cover this repair.
Integration CI is a separate claim: queued or cancelled parity jobs do not
establish that the chain is green.

## Cross-instance arrow values

The arrow repair and four new native tests are committed at
`a919cdb923953550b5cec92739b77d717c74e112`. No pre-arrow baseline pass is
extended to this revision. Its scoped two-package all-target check and all eight completion tests passed
with exit zero. The two compiling production controls each exited101 at their
named native assertions and exited0 after restoration, with matching original
and restored production hashes. The foreign-origin control failed all four
new native tests; the redundant self-edge control failed the explicit target
release test. These measurements are separate from the pre-arrow baseline. The prior implementation at
`5ce7122d895f27483e7de0b7228a75d2a6e0afca` failed all four test-only
desired-behavior supplements at named native assertions, with actual wrong
receiver values and lost origin lifetime. Removing the supplements restored the
original four completion tests, exit zero, and the original test-file hash.

The two new production controls deliberately rebind a foreign arrow to the
reading receiver and retain a redundant self receiver edge. Each is required to
compile, fail its named native assertion and pass after byte restoration. The
first recipe for the separate Base wrapper control did not compile because the
mutation left a venue argument unused; it is retained as an invalid recipe, not
a discriminating pass. A corrected compiling recipe failed the virtual getter
assertion with exit101 and restored with exit0.

## Mapping and current limits

The narrow decoder classifier initially failed at
`a919cdb923953550b5cec92739b77d717c74e112`, first stopping on
`unclassified inherent decoder classes::CoinbaseExchange`. The initial receipt
expected the later helper owner instead; it records that correction and does
not claim the later classification was independently reached by that run.
At `bee372473352e5b5489e71e9543f8ed7ce01423e`, the named inventory test passed
with exit0 after exactly three non-wire storage operations were classified and
the obsolete raw-slot Clone allowance was removed.

At `5801e6717a6c8f47abb297896edf9665da561b37`, scoped formatting, all fourteen
Node declaration/audit tests, the actual notification walk and the extractor
check exited0. The extractor reported zero updates, additions or orphaned
symbols. The walk retained all262 reviewed roots and resolved all262 source
declarations:247 currently ported callables, fifteen notification finding roots,
thirty reviewed finding paths, thirteen content-pinned direct calls and zero
unhandled ported paths. The actual Rust/API-oracle source and completion tests
are byte-unchanged from the arrow gate; the scoped git diff exited0, with paths
and revisions recorded in mapping-census.json. No full post-arrow venue run is
claimed from this equivalence.

The actual invention/status checker still exits1:5895 diagnostics at the
integration parent `d4e20845a85cc4e2711c99648ef7547f8aa806df` and the exact
same5895 at the mapped revision. The checker source is byte-unchanged across
those revisions. The original branch had97 additional diagnostics; the repair
removes that entire delta. The parent and current results are recorded, not
represented as a green parity job. Diagnostics span shared crypto/exchange and
other venue/worker manifests, so this is a cross-area inventory failure.

Tomllib measures392 ported,34 divergent,0 pending in426 Coinbase rows across
sixteen manifests at the mapped revision. There are seventeen canonical FINDING
notes: the two original class findings, fourteen prior notification findings and
one newly discovered class snapshot publication gap. Exactly one row changes
status in this unit:ported to divergent. This is an implementation-review
withdrawal, not the deferred explanation-only audit.

NOT RUN at the post-arrow revision:the complete venue all-target test runner,
workspace check/build, full workspace host clippy, and the full wasm CI matrix.
Earlier baseline venue/scoped clippy and broad workspace receipts identify their
own measured revisions and are not extended to this repair. Integration parity
is unconfirmed; queued or cancelled jobs never count as green.
