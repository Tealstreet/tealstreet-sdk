# Request checker precision for pure data macros

Owner: codex-uufub4. Reviewers: codex-hcmq3f (bounded semantics/native proof)
and claude-8x5tm1 (gate integrity). Base: actual Rust landing035948b564a,
which includes #2165. Branch: fix/rust-request-macro-provenance.

The overseer explicitly ruled the13 WEEX account occurrences false positives:
all six HTTP endpoints already use managed SelectedRead. This is a separate
checker PR, not the local account-custody audit and not a venue cutover.
Status: both reviewers cleared the corrected immutable preview on actual
6fb44235f3e. The exact checker and production-manifest patches are applied;
all46 Python controls and full bans/ratchets pass on the reviewed base.
Composed publication gates remain required. No venue, Rust runtime or build
input changes.

## Exact problem

context_fingerprints retains a macro in an escaped-Cx scope when any argument
token has a spelling in METHODS. That treats the local data variable `request`
as if it were a legacy request expression. It freezes matches!(request.kind,...)
and format!("limit=100&page={}",request.page), despite neither invoking a sender.
The direct Cx-call inventory and its taint analysis are independently necessary.
They must remain intact.

The unchanged f895 account manifest is8 fingerprint rows/13 occurrences. All13
were independently reproduced from whole source with the existing tokenizer,
group_end and canonical payload hashing. Each occurrence's `request` resolves
to the existing typed non-Cx parameter (`other`) in the appropriate function;
no macro argument references a resolved Cx/unknown closure parameter. The macros
are in send_account, account_item, account_response, log_skipped_rows and
account_failed. Exact per-occurrence evidence is retained in
`/tmp/codex-weex-account-context-plan/inventory.json`.

## Proposed closed precision rule

Change only the extra macro-fingerprint predicate in context_fingerprints.
Preserve the earlier direct/UFCS/raw/generic call accounting, counted-call
wrappers, escape fixpoint, Cx binding resolution, test partition and immutable
baseline ratchet. Preserve plain fingerprints(), which the socket scanner also
uses. No endpoint, venue, hash or variable-name allowlist.

A macro can lose ONLY its extra conservative fingerprint when all of these
conditions are proved:

1. Only bare matches! and format! are eligible. Qualified names, aliases,
   visible imports naming either macro, local or ancestor macro definitions,
   macro_use/export and unresolved macro identity stay conservative. Spelling
   alone does not prove a standard macro. No other nested macro is eligible.
2. Every METHODS-spelled token must be an enclosing fn parameter with an explicit
   concrete non-Cx type, not generic/impl/dyn/inferred or a closure parameter.
   Its expression is only IDENT followed by .field segments, with no call,
   index, turbofish or parenthesized receiver. Every rebinding in that function
   refuses proof except the closed later-nested-let exception below. No flow
   analysis or borrowing an outer parameter proof across ambiguity.
3. For format!, arguments after the literal contain only these references,
   literals and the tokens . , &; no parentheses or calls. For matches!, the
   scrutinee obeys rule2, and parentheses are allowed only in the pattern after
   the first top-level comma. No resolved Cx or unknown capability is passed.
4. Any actual counted direct/raw/generic/UFCS/indirect call retains its wrapper
   fingerprint, even beside a pure sibling. This never clears fn taint or raw
   call rows. Unknown forms retain the old accounting.

### Closed later-nested-let exception

Both reviewers supersede any broader binding-scope proposal with ONLY this:
an ordinary fully parsed let/let-else may be ignored when its strict nested
block opens AFTER the candidate macro closes and cannot contain the invocation.
Prove balanced statement/block and module/closure/item boundaries, the actual
pattern binder and every potentially overlapping binding. Same-block later
bindings stay conservative, as do all for/match/if-let/while-let/closure binders,
macro-generated bindings and unresolved patterns/scopes. No reachability or
type inference. The implementation may support fewer structurally proved blocks.

In send_account, all five data macros precede the strict nested queue-error arm
with let Task::Account(request) = pending.task else { unreachable!() };. Pair this
positive with raw calls/wrappers inside that arm, same-block-later, earlier or
containing blocks, invocation inside the arm, a second overlapping binder,
closure and ambiguous pattern negatives. Record every ignored binding's actual
scope/token bounds in the removed-row ledger.

For account_response, split each arm's pattern from its guard at a balanced
top-level boundary. A guard-only request reference is not a binder; nested guard
let/closure ambiguity remains conservative. Pair actual-pattern and guard-shadow
negatives. This parser correction grants no blanket guard exemption.

### Closed single-hop private-import proof

At each invocation, consider only imports in the same module or a containing
block/fn scope, regardless of source order. Imports inside other functions or
sibling/child modules do not reach it; cfg(test) alone is not a scope proof.
Ambiguous brace/item boundaries refuse precision. A visible glob normally
refuses precision. The sole exception is a top-level, flat, single-segment
`use crate::<ident>::*;` in the consumer. Resolve it via actual tracked plain
`mod <ident>;` declarations to exactly one source file, without path/conditional/
ambiguous declarations. Record the consumer/target module paths, declaration,
mod-chain files/blob hashes, enclosing scope and target blob. Grouped, external,
self/super, nested or otherwise unresolved visible globs remain conservative.

Ordinary use items in ancestors are not inherited as bare names in child modules;
no traversal or exception is inferred for lib.rs use wire::*. Textual macros are
different: any macro_rules or macro_use on the actual root-to-parent mod chain
refuses precision, as do unresolved item-generating/ancestor macro hazards.

The target has no macro definition (macro_rules or decl_macro), macro_use/export,
extern crate, include/item-generating source, nested module or re-export of ANY
visibility. A use item is private ONLY when it has no visibility modifier:
pub, pub(crate), pub(super), pub(self) and pub(in ...) all refuse the proof.
Private target imports, including private globs, are permitted and are not
traversed as exports. The consumer must be a sibling or unrelated module in the
plain mod graph, NEVER the target or its descendant: descendants can see private
imports. Ancestor macro definitions/imports/macro_use still require independent
resolution, and unknown cases keep old counts. No venue/hash allowlist.

This is the co-signed hcm/8x supplement, supported by the pinned Rust1.99 private
import failure and public-glob working probe. It permits originals and an exact
unapplied source preview; source and every removed-row proof still require both
reviewers before application.

### Consumer item-generation refusal

An opaque macro in the invocation's module or a containing block may introduce
standard-macro shadows or imports without a data argument. Refuse the purity
proof for those invocations independently of METHODS-spelled arguments. Unknown
position/scope stays conservative, including expression positions this narrow
implementation does not classify. Use the same proved lexical module/block
separation; unrelated functions and sibling/child modules do not become visible.
No venue/hash/generator-name exemption. Both source-local and tracked-module
controls pin named-import install/shadow/import generators with empty arguments,
module/same-block and after-invocation cases, ordinary/explicit-shadow working
controls, disjoint scopes, raw wrappers and the later nested-let proof.

## Originals and hostile controls

Append controls to the existing Python checker tests. Preserve every existing
body/assertion and both production/test inventory partitions. No Cargo build.

- Genuine escaped-Cx fixture with typed Request parameter: the exact WEEX
  matches and page-format shapes must add no extra macro row. Keep the Cx
  escape independently present, so a GREEN from an untainted scope proves
  nothing. Exercise every one of the8 fingerprint payloads/all13 occurrences.
- In those SAME shapes, insert a real raw cx.request/request_streamed/body,
  sign_http/done/fail, raw identifier, generic and UFCS call. The actual call
  and its legacy wrapper stay inventoried with unchanged payload/count. Mixed
  pure and raw siblings cannot let one another escape or rename a frozen hash.
- Unknown macro selector, local/imported macro replacement, nested unknown
  macro, a Cx named request, inferred closure request, shadowed data binding,
  and moved/borrowed Cx arguments retain their conservative inventory or typed
  refusal. Real macro-generated sender and include! still fail closed.
- Flat local-glob private imports versus pub/pub(crate)/pub(self)/pub(in ...)
  re-exports, exported macro, macro_rules/decl_macro, macro_use, path or conditional
  declarations, missing/ambiguous targets, nested modules, grouped/external/super
  globs and descendant consumers. Record actual declarations and target blobs.
- Raw identifiers in macro imports/definitions and all binding forms obey the
  same identity as ordinary spelling. Implicit format captures and named-width
  selectors cannot hide a Cx/unknown capability inside the atomic literal.
- Working controls for literal method-like text, ordinary typed data references,
  resolved Cx members and non-Cx fields; both default source-local index and
  workspace escape index. Existing taint, ownership and baseline tests pass.

Record original outcomes before changing the checker. Assertion REDs must be
the intended extra false-positive rows, not setup/compile/import failures.
All deeper hostile branches must execute in source qualification.

## Cross-venue proof and inventory application

Run the unchanged canonical production/test scan on actual base and the exact
candidate. Save the complete before/after inventories and per-path/per-hash
counts. The diff must contain removals only. For EVERY removed row, record:
path, count, original token payload, macro kind, parameter binding and the
structural proof above. Explicitly list every non-WEEX removal, or state that
none occurs. Do not assume that removing13 WEEX occurrences is the whole diff.

The WEEX account block may disappear only when the actual canonical candidate
scan verifies all8 rows/13 occurrences absent. On actual #2165 base, removing
only that block gives WEEX6 remaining rows/7 occurrences; other potential
removals require their own explanations. No signing/credential/socket manifest,
Rust source, endpoint, macro spelling or provider change is allowed. Update
request production/test manifests only for actual justified scan removals;
foreign rows and order remain byte-exact except individually reviewed removals.
Maintain exact-current-map equality and per-parent/base/event shrink checks.

## Qualification and publication

Read-only plan review precedes source. Then execute original controls, review
an immutable exact checker preview plus cross-venue ledger, apply the reviewed
shape, rerun all Python checker tests and full bans/ratchets, and verify the
closed diff. Obtain hcm source/preservation and8x gate-integrity CLEARs before
one frozen PR push/run and READY. Merge actual origin/feat/rust-rewrite before
qualification; never master, rebase or force-push. Exact-tree GO precedes landing.

All Rust, Cargo.lock, profiles, hosts and build/workflow inputs remain unchanged.
No shipping WASM size delta is claimed or inferred from Python test results;
8x explicitly ruled no size measurement necessary for this Python-only change
with runtime/build inputs unchanged. State that scope in the PR; do not relabel
earlier #2165 measurements.

## Reviewed application

The corrected checker SHA256 is
`fbc95231bfcd5be033769bbef10b174586d7fdd1c55fa3389347f9cf4e926b24`;
the exact source patch is
`5ec4bf2b630ee308f6a21f4084ce9f7239a1c369eb4631cb963c7463c681740a`.
Hcm semantics and8x gate-integrity independently cleared this corrected
preview; the earlier opaque-generator preview was withdrawn and is superseded.

The canonical removal-only ledger decreases13 fingerprint counts by18
occurrences: WEEX account8/13, WEEX history one occurrence (same retained
fingerprint2->1), BingX trading2/2 and Bitrue account2/2. Twelve keys disappear
completely. WEEX retains6 fingerprints/6 occurrences. Every removed payload,
parameter binding, enclosing scope and tracked-module proof was reviewed.
The test inventory and signing/credential/socket manifests remain unchanged.
