# Request checker precision for pure data macros

Owner: codex-uufub4. Reviewers: codex-hcmq3f (bounded semantics/native proof)
and claude-8x5tm1 (gate integrity). Base: actual Rust landing035948b564a,
which includes #2165. Branch: fix/rust-request-macro-provenance.

The overseer explicitly ruled the13 WEEX account occurrences false positives:
all six HTTP endpoints already use managed SelectedRead. This is a separate
checker PR, not the local account-custody audit and not a venue cutover.
Status: plan first; no checker, runtime, test or inventory changes yet.

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

1. It is the ordinary matches! or format! shape, not an unknown macro, alias,
   imported replacement or locally shadowed macro definition. Ambiguous names
   retain the old conservative result. The exact treatment of glob imports is
   a source-review pin: no hidden macro/Cx authority may be inferred from them.
2. Every METHODS-spelled token causing the old false positive is a plain data
   reference to an already typed non-Cx parameter, with ordinary field access
   as in the13 working WEEX cases. A member/path/method reference, direct or
   indirect call, raw identifier sender, alias or unresolved binding is not
   eligible. Local let/for/match/closure shadowing must not borrow the outer
   parameter's proof; ambiguous cases stay conservative.
3. The macro passes or captures no resolved Cx/unknown closure binding. Its
   source contains no counted legacy call, nested unknown macro, callable
   selector or other unresolved capability transfer. Do not clear the enclosing
   function's existing taint or use the data proof for another expression.

The implementation may refuse additional uncertain shapes instead of growing
Rust name or type inference. It must accept all13 exact WEEX cases for a stated
structural reason. The reviewers must settle the concrete shadow/import
predicate before source application. This plan does not authorize broad removal
of arbitrary METHODS tokens from unknown macros.

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
this checker-only slice proposes no release build. Any additional measurement
requirement is for the auditor to disposition, with exact unchanged runtime
input proof, rather than relabelling earlier #2165 measurements.
