> Historical evidence: the test-only V3 comparison/canonical bodies, operation holders and their oracle commands are retired. The live legacy operation shape is retained; see CLAUDE.md.

WOO public parity core — nine rows

Gate SHA: `039d00875a6265e5c3b8f753e57a5b39d7ac6593`. Fresh branch `feat/rust-woo-public-parity-helpers`
from integration `c4caf76e6cdb22b858492ca47796fcb46a212d9e`.
No unrelated feature branch is an ancestor introduced by this slice.

Moved **9 pending rows: 1 ported, 8 divergent (MIXED)**. Independent tomllib
expectation lists the nine symbols in `scripts/public-parity-core-census.py`.
At the gate SHA, assigned public-parity/runtime-selector is **4 / 18 / 28**;
all tracked WOO manifests are **169 / 298 / 65** (ported / divergent / pending).
Only `woo.v3.public-parity.ts.toml` changed. Census and behavioral proof are
separate claims.

Measured gates at that SHA (exact commands and output excerpts in
`PUBLIC_PARITY_CORE_GATE_RESULTS.json`):

- Actual TypeScript source oracle: exit **0**; private helper exports added to
  transpiled actual source. No helper reimplementation. The unused candle mapper
  import is denied if invoked. No TypeScript typecheck claim.
- Independent tomllib census: exit **0**.
- `parity:extract` writer and `parity:extract:check`: each exit **0**; text says
  Updated/Would update **0 manifests; 0 added, 0 orphaned symbols**. The writer
  was run after each manifest edit; no file array was hand-spelled.
- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-woo
  --all-targets --jobs 2`: exit **0**. This is scoped compilation.
- Scoped WOO tests (`--lib --test runtime_selector --test public_parity_core
  --jobs 2`, TypeScript path set): exit **0**. The output excerpts name the
  runner's measured test floors: library 11, public_parity_core 5,
  runtime_selector 5, stopping after these three targets (21 tests). The full
  WOO suite was not run.
- Four-control driver: exit **0**. All four compiled mutations returned **101**
  at their named native assertions and each restored test returned **0**.
- Restored native source/tests/scripts/manifest diff against gate SHA: exit **0**.

Controls stop at iteration **4**; none survived. They are independent production
mutations: symmetric route serde names, signed-zero SameValue equality,
Map key insertion order (OTHER before 42), and blockers-first result priority.
See the committed JSON for exact test names, assertion messages, red/restored
exits, commands, and failure excerpts. Each overlay was removed byte-for-byte.
No exhaustive mutation proof is claimed.

Native assertions precede any output encoding; there is no native record
encode/decode roundtrip. Route labels decode to independently named enum
variants. Diagnostic records assert native kinds, presence, values, binary64
bits, provenance, key order, first winner, callback order/error identity,
result status, owner identity, shallow element identity and slice copies.

Enumeration floors: five route variants; eight Object.is pairs; six keyed
mismatches with twelve callbacks; four result cases; five source-returned UTF16
records with independently expected 3/3/1/3/3 string slots. These are floors,
not completeness of arbitrary JavaScript values, accessors or collections.
The UTF16 witness verifies actual returned U+D800 units before projection,
then decodes each actual source string into native String (refusal). Whole
source records are refused by serde_json::Value ingress before the existing
JsJsonValue::from conversion can run. No JsJsonValue or parity-record serde
deserializer is claimed. Ordinary strings, scalar pairs and literal
backslash-u text decode exactly. Per-row notes cite the canonical string
boundary and retain specific CHOSEN causes.

Native parity preserves closed route labels, shallow readonly diagnostic
ownership, absent vs undefined/null blocker values, NaN/signed-zero/compound
identity, first-wins duplicates and Map insertion order, stored undefined as
missing, generic caller error identity, blockers-first priority, and borrowed
legacy snapshot identity/live interior state. Documented native boundaries
are immutable diagnostic compound data and fixed slices/metadata/typed context.
Actual-source counterexamples distinguish live compound aliases, for-of
appends, callback metadata mutation, context overrides and custom result
iterators; native assertions show each chosen boundary.

Sub-agent review `/root/context_review` found no remaining supported-domain
semantic blocker. Its two proof additions and manifest citation correction
were applied; reviewer ran no gates.

Earlier dirty diagnostics: scoped check exit **101** (extra reference layer,
fixed before this SHA); native preflight exit **0**, not used as SHA evidence.
A scoped check at `882306c7b0a6924527214bb65da39c2c69bbb4bd` returned
**101**: the test attempted an unimplemented JsJsonValue serde decoder.
`039d00875a` corrected the test to actual Value ingress plus From conversion;
the failed SHA and its measured outputs remain in the committed results.

NOT RUN: full WOO suite; workspace check/build/tests; host clippy; WASM
check/clippy/tests; TypeScript typecheck; live transport; CI. No shared type
field or shared trait method changed, so the mandatory workspace sweep trigger
does not apply. No PR opened; nothing merged into integration.

Post-gate integration merge

Integration `2a0e1813663e4b5bb627bfde9db765cd958c5d84` merged down cleanly at
`b7e9fff220df669793904596e6532bfdcab57d1b`. Native src/tests/scripts, local Cargo manifests/lockfile,
direct crate dependencies, assigned TypeScript sources and WOO manifests are
byte-unchanged from `039d00875a6265e5c3b8f753e57a5b39d7ac6593`: the scoped `git diff
--exit-code` returned **0**. Native cargo/test/control exits remain bound to
the original native gate SHA; they were not re-run after this unchanged merge.

Global manifest/exemption inputs changed, so fresh writer and extractor-check
at the merge SHA each returned **0**, with green zero-update/add/orphan text.
Fresh tomllib census exit **0** retains assigned **4 / 18 / 28**, WOO
**169 / 298 / 65**, and nine moved rows (**1 ported / 8 divergent**).
Exact merge commands and excerpts are committed in the results JSON.
The evidence commit after this merge changes documentation only. No unrelated
unreviewed feature work was introduced as an ancestor by this lane.
