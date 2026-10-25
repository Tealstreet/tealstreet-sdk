# Phemex late cancel repair: final four legacy request sites

Status: consumer PLAN ONLY, based on landing tip 035948b564a9338371be2b2aef4cad073e7302a9. No runtime, original-test, Cargo, manifest, budget or CI change is authorized by this artifact. Reviewer: claude-fe7lgm (core/API and custody), with claude-dazzy3 as alternate. The independent Bitget history PR #2170 remains frozen on its own branch.

## Scope and prerequisite

Migrate Phemex cancel_late -> late_next -> dispatch(None, ...) to engine-issued CancelOnly repair authority. Remove exactly four fingerprints / four occurrences; Phemex request inventory then reaches zero. Preserve ordinary command Plans, all read roots, signer recipes, socket lifecycle, financial reducers and native cancel grouping.

| File on 035948 | Fingerprint | Count | Existing expression |
| --- | --- | ---: | --- |
| venues/phemex/src/lib.rs:326 | fc8a5b682da002c9a8b697b45ebe204db49576e4cd13a275d46a52c7e13f01e6 | 1 | cx.request_streamed_body in the orphaned legacy request helper |
| venues/phemex/src/lib.rs:328 | 34b6cbe60cecfea179e31290b2eb916f11a7555598d45b795b4633e6afc768fc | 1 | cx.request in that helper |
| venues/phemex/src/lib.rs:331 | 6cf35cca09778b64b2c0236671ce5549e5aa10a38a919d5d30317ddcfc43d219 | 1 | cx.sign_http in that helper |
| venues/phemex/src/trading.rs:1313 | e6f9b79a027f31adbd5a2b4c25669770848c36a0b761835a87f5783a961f4790 | 1 | self.request in late-only dispatch |

Independent context_fingerprints production analysis equals the exact 035948 manifest for both files (inventory.json). The streamed branch has no remaining live late-repair caller; delete the entire helper rather than retain a fallback. Other venue inventories remain incoming-exact.

Shared authority is the landed design docs/plans/RUST_REPAIR_MUTATION_AUTHORITY.md. Concrete reviewed API pin is #2151 d46fc393cb64c67204ce858129fbca0f7151d991, tree 6c002769ff411cc9f484fe1d2d668dab7ef050bc; actual #2151 landing was independently checked as 8f5f9a2f95701ad40fcef19e3738423b587c4f98 during this audit. Before source, plain-merge the actual landing tip and check its engine API rather than assuming this plan's base contains repair-mutations. fe7 owns every core change; this consumer does not edit engine.

## Native behavior that remains the contract

1. Unknown placements enter account.reconcile with a client ID and discovery generation. An order already visible when cancel-all dispatches is covered by that command's immediate cancel steps; only absent owed client-ID generations are added to the late-cancel latch (account.rs:1430-1463).
2. Both orders_cancel_all callers are synchronous inside trade_command: the empty immediate-job branch (trading.rs:1164) after begin_preflight and before done_empty_plan, and the nonempty branch (:1226) after successful admit_serial_plan. The genuine command entry is Bound at each; later Plan/timer/ready callbacks cannot mint.
3. Unknown placement reads wait for actual raw signing expiry + 5000, plus the existing pause/placing gates. A private-stream observation may reveal the owed order earlier and causes immediate repair. Do not add a receipt deadline, entitlement age limit, aggregate root clock or new wait before a valid stream observation.
4. cancel_landed accepts only the same symbol, an owed client ID, no parent_id, and Open/Updating for cancellation. Positive fills retain the manual-position diagnostic. Duplicate observations consume no second cancellation. Later placements/generations outside the recorded cancel-all set remain unrelated.
5. cancel_plan normalizes and deduplicates native IDs, resolves cached order/native symbol/position side using the existing precedence, and builds stable first-seen groups by native symbol plus position side. Chunks are 10. A one-ID GROUP uses /g-orders/cancel; a multi-ID GROUP uses /g-orders, even when its final chunk has one ID. Preserve encoded query order/bytes and no body.
6. There is one active late-cancel request across the venue. Groups run FIFO. The next independent group runs after success or failure, with no automatic resend. Origins are absent from native grouping: compatible members from two cancel-all commands share the same native request, rather than being split by entitlement.
7. Native dispatch marks before rows Canceling, advances account order revision and then sends. Preserve these optimistic effects only AFTER repair admission succeeds. Failed ACK cleanup restores only still-matching Canceling rows under the existing amount/fill/revision guards; never overwrite newer stream state.
8. Successful cancel updates the same accepted/finalized rows and warning, then account refresh and the next FIFO group. Failed cancel emits CHILD_LEG_FAILED with native manual-cancel wording, refreshes, and continues the next independent group. Repair never emits CommandDone/CommandFailed, reopens the old command or changes its journal.
9. A native bulk object may acknowledge the whole requested batch while giving final row status for its representative only; array replies prove individual IDs. Preserve that documented codec rule. For malformed/partial/duplicate ACKs retain each proven accepted ID and mark the rest Unknown; never manufacture an all-accepted fact.
10. Complete, current, postdating reconciliation removes only lost/cancel generations it covers. Incomplete, failed, stale or superseded reads do not prove absence. A child observation, unrelated client ID or a newer placement cannot discharge the old entitlement.

## Consumer authority and execution

Keep a venue-owned entitlement table separate from transient reconciliation entries. Each owed (symbol, client ID, generation) maps to one engine-issued Entitlement and Subject; native Reconcile.cancel entries retain or reference those exact subjects. Repeated cancel-all must reuse already-covered subjects, not remint the same unresolved placement into a second grant. A new cancel-all mints only uncovered owed subjects, then records their latch. No owed subjects means no mint.

At each genuine Bound cancel-all entry, call mint_command_repair(&preflight, Vec<Subject>) after native gates and successful admission. In the nonempty case serial Plan admission stays first. In the empty-physical-job case mint the nonempty owed set before done_empty_plan. Primary comes from the admitted WriteAfterReads Preflight; do not reselect from discovery, borrow a read key or manufacture a command.

A mint Capacity refusal follows the approved additive CommandError plus one Warn from the core; the original Plan's slots, journal and terminal remain unchanged. Keep native owed diagnostics, but never store spendable authority on a refused mint. Other refusal variants are explicit control/source gates; no raw-send fallback, synthetic entitlement or silent remint at discovery is permitted.

Discovery still uses native account/stream reducers to publish the exact order first. Native cancel_plan builds the existing ordered groups/chunks. For each queued cancel step retain its original Subject and Entitlement references and exact native-ID binding. Call admit_repair_cancel(&[(&entitlement, &subject, Binding { order_id, symbol, side })]) outside any command entry. Binding.symbol is the Store's native-normalized SDK symbol; side is the adapter's existing Merged/Long/Short key. URL/query still use native_symbol as today. Never mix those two symbol representations.

The core atomically checks <=10 subjects, common engine/installed Primary, command origins, symbol/side, Store client-to-native membership and unspent lifecycle. Cross-origin groups use one joint root. One invalid member refuses the group without spending anything; preserve the native manual-cancel diagnostic and FIFO cleanup, with no automatic retry or alternate authority path. Pin the exact refusal/latch outcome rather than inventing a successful repair.

After Ok((RepairRoot, Context)), retain that exact pair and the native before/revision state. Mark Canceling/account_write only now, and use request_signed_managed(context, None, DELETE, native URL, None, existing dispatch_signing::Http recipe, Repair pending). The signer identity must own context.credential's installed Primary reference; source review checks identity/ref equality. No change to HMAC, expiry, Content-Type or key-material custody. Remove legacy dispatch and request helpers once this is their last caller.

On response, first match the exact active RepairRoot/Context and use validate_http_context only as a live-issuance check; it is NOT an exact-DATA validator. Purely decode the native cancel ACK once, preserving representative-object versus array semantics and explicit accepted IDs, without Store/counter/publication effects. Record the definitive/partial/unproven outcome through http_write_fact (or reviewed http_write_result), whose exact delivery check must succeed, then validate_http_write_effects before applying the native Store effects; finish_http_context retires the leg. Factor the existing combined decoder/effect reducer within this venue only, with financial/wire assertions unchanged. Keep exact typed provider/local/physical-timeout cause on the failure path. Released 408/5xx/integrity may seal in the engine without venue DATA; never decode, resend or grant a new root from those bytes. If the concrete Repair owner does not support these shared fact/effect gates, report that API gap to fe7; do not invent validate_http_delivery or treat liveness alone as DATA.

Engine RepairNotice::Settled is the ONE cleanup/FIFO event for every admitted root, including preparation refusal and engine-owned unproven sealing. Match exact RepairRoot issuer/id, remove active work once, preserve accepted effects, restore only matching refused/unproven optimistic rows, emit the native warning or manual-cancel error once, and run native account refresh plus the next FIFO group from this reconciliation Cx. A response callback may stage decoded native details but must not independently advance the same queue. Do not wait for a later user command. Unknown cancel uses the core's reconcile-only behavior; it does not fabricate a Place uncertain scope or reuse the original placement's signing expiry as a new cancel fence.

The core's per-call bound is 20,000 ms / Never and covers preparation, signing, queue and physical release/response. Native hosts already default physical HTTP to 20,000 ms, so CALLERS must state the extra pre-release coverage. There is no aggregate root/entitlement wall-clock bound and no native retry. Original sealed commands remain isolated even after receipt expiry or numeric ID reuse.

## Lifecycle proof: reviewed fit and loaded-orders proof pin

Fe7 cleared the concrete Repair fact/effect gates at actual 8f5f9a2f957; no shared seam is missing. Liveness is validate_http_context, exact DATA is checked by http_write_fact/http_write_result through journal_owner and root-step intent, Store effects require validate_http_write_effects, and finish_http_context retires the leg.

Keep the venue-owned entitlement table separate from native reconciliation removal. Unspent terminal subjects use retire_repair_subject(&entitlement, key, Retire::Terminal) only after Store publishes Closed/Canceled. Spent subjects remain until exact root settlement and later terminal/definitive resolution permits retirement. Empty repair_subjects is not proof that spent Unknown resolved.

Per fe7's lifecycle disposition: begin_orders_snapshot at genuine open-order Read issuance and carry its token with that Read. Inside exact accepted DATA, BEFORE finish_http_context, call commit_repair_snapshot(read, [token], |current| merge(current, parsed)). The pure merge is existing accept_write_order behavior over current rows, so newer stream rows remain authoritative; this commit IS publication and REPLACES the individual upserts. It is not an extra no-op commit after publication. Native account metadata bookkeeping stays synchronized with the accepted rows and existing revision guards.

If commit returns None (stale selective-fence token, or no live accepted DATA), retain the existing native incremental publication behavior for a delivered response and DO NOT retire any subject. Failure callbacks publish no parsed response rows and obtain no receipt. This is a stale-observation branch, never a legacy transport/admission fallback. After a successful commit, retire only absent subjects of read.symbol whose generations that complete read covers, before the Read retires. Incomplete/pre-mint/stale/other-symbol proof never retires.

Loaded-orders source proof: the ONLY grant-mint call sites are CancelSymbolOrders' orders_cancel_all paths. Its planning gate at trading.rs:1009 requires account.orders_ready(); account.rs:220 requires authenticated && snapshot && open. account.snapshot becomes true only at account.rs:1317 AFTER either safe_snapshot set_orders(staged) (:1307) or unsafe snapshot set_orders(current.clone()) (:1315). Both set Store.loaded.orders true. No late entitlement can therefore originate before that full-load gate. An early unknown-placement reconciliation without cancel-all has NO entitlement, and must retain native incremental publication rather than call the loading commit seam.

Implementation guards the proof: use the receipt commit only for relevant held repair subjects when Store.loaded.orders is already true; otherwise use native incremental publication and no retirement. Do not add an admission wait or new full-load requirement to ordinary early reconciliation. Pin actual cold-start/early reconciliation leaving loaded.orders false, then a genuine full snapshot -> unknown placement -> cancel-all -> reconciliation showing it remains true. If any real path disproves the mint/full-load invariant, stop and return to fe7 for a non-loading commit variant; this venue does not add one.

Disclose and pin the reviewed publication deltas: Store orders_revision advances once per committed read instead of once per accepted row, and commit sets changes.orders even if its complete merged rows are unchanged. Keep the separate account.order_revision and revision-dependent cancel rollback logically correct (changed versus unchanged); test concurrent newer stream/fill updates and both empty/one/many-row reads. CALLERS must name per-read publication and unchanged full-load semantics. No empty commit is invented solely to obtain authority.

Additional lifecycle controls: an other-symbol subject is not retired by this symbol's receipt; a token begun before mint returns Stale; mixed accepted/refused/Unknown roots retain exact membership and never remint; terminal and complete-read retirement release grant capacity only for proven subjects.

Root settlement and Retire::Terminal/CompleteRead preserve the core's fixed-engine ownership: identities/signer are frozen after start; the concrete core has no install-generation or engine teardown API. Do not implement the obsolete draft counter/teardown promises or an account epoch reset the host does not expose.

## Original controls to freeze before runtime changes

Add one focused target, tests/late_repair_context.rs. Drive genuine Unknown placement -> admitted cancel-all -> native order observation. Provider/route tests must use actual Prepare/Bind/Claim/release; do not forge a Context, entitlement, receipt or root. Keep existing tests/trading.rs and dispatch-signing vectors unchanged.

| Control | Original / source requirement |
| --- | --- |
| Managed late repair | Genuine late DELETE waits for actual route preparation; zero Action::Http or legacy Sign before Prepare/Bind/Claim. Intended original assertion RED on 035948. |
| Admission before optimism | Exhaust real owner/repair capacity; no Canceling publication, account revision effect or raw repair HTTP when root refuses. Original RED from native unmanaged dispatch. |
| Exact source identity | Primary+Secondary host signs repair only with Primary; Secondary-only/public-only cannot mint writes; real recipe/URL/query proof. Native positive plus source gate. |
| Both mint sites | Empty immediate job but owed unknown placement, and nonempty immediate cancel Plan; mint after genuine admission, preserve original done[]/IDs. No owed set mints nothing. |
| Mint refusal isolation | Saturated mint after successful Plan admission adds one diagnostic/Warn, retains Plan result/journal/credits, and authorizes no later send. Pin exact text and terminal count. |
| Native group bytes | Cross-origin same-side subjects share one request; Long/Short stable FIFO; 10+1 in one group uses /g-orders for BOTH chunks; singleton group uses /g-orders/cancel. Preserve duplicate-ID normalization. |
| Any-age stream observation | Genuine owed old placement seen on private stream repairs immediately before post-expiry read; repeat observation sends nothing. No arbitrary age cutoff. |
| Post-expiry discovery | Actual signing raw expiry+5000 controls read time; pre-expiry timer inert; unrelated/child/new-generation rows untouched; fills keep manual-position warning. |
| Exact ACK effects | Representative bulk object, complete array, mixed/duplicate/missing rows; accepted IDs stay ordered/proven, unproved members Unknown. Guard newer revisions/fills during response. |
| One cleanup and FIFO | Success, rejection, Network, physical timeout, provider refusal, released 408/5xx/integrity all clear active root once; next independent group advances without a user command; no resend. |
| Liveness and stale delivery | Preparation held through 20s equality, late bytes and replayed old IDs cannot publish or settle next root; no aggregate entitlement timeout. |
| Subject lifecycle | Complete postdating accepted read retires absent covered generation; stale/incomplete/pre-mint/failed read does not; terminal row retires its own subject; spent Unknown never remints. Uses the reviewed lifecycle publication plus loaded-orders proof above. |
| Original command isolation | Done/expired/reused-ID cancel-all never receives repair terminal, progress or changed evidence; fresh command entry survives cleanup; foreground/repair conflicts remain enforced. |
| Credits | Saturation refuses atomically; terminal/preparation failure/physical timeout free exact leg/root credits; 255 live peers survive; repeated groups return full capacity. |

Native working references: transport_timeout_cancel_all_goes_out_now_and_cancels_the_late_landing_after_expiry; failed_late_cancel_is_reported_as_an_error; late_landing_seen_on_the_private_stream_is_canceled_immediately; late_cancels_in_two_groups_are_sequenced_without_false_errors; placement_lost_after_cancel_all_is_not_canceled_by_it; existing complete-read, signed-expiry, partial-cancel and newer-stream tests. Re-run originals on the unchanged base and record genuine assertion RED/GREEN counts, not an expected tally.

## Implementation, measurements and landing gates

Expected consumer scope: Phemex account.rs (subject/lifecycle bindings), trading.rs (mint and managed repair reducer), lib.rs/Pending callback plumbing, Phemex Cargo repair-mutations activation, one new test target, docs and four exact request-map deletions. No generic engine change; any missing API is reported to fe7/overseer rather than added here. Remove orphan legacy Request/Pending branches only where every caller is proven migrated; preserve shared command cancel decoder and financial behavior.

First source commit: measure Phemex family AND local combined on the same actual composed landing tip before/after with unchanged harness, Rust/bindgen/Binaryen and jobs4. Enabling repair-mutations makes the core reachable, so no zero-byte or prior-Phemex number is inherited. Estimate is unmeasured; credit only measured bytes, use the then-current Phemex/combined caps, and refer unexpected growth to the overseer. CI records family bytes; it does not record combined. If any generic engine source change becomes necessary, stop consumer scope and obtain owner review plus paired Bybit/Bitget/combined on that separate core change.

Sequence: plan/API review -> originals freeze and execution on unchanged native base -> consumer source -> exact source/custody review -> full native Phemex/strict clippy/fmt/bans/ratchets and exact map4->0 -> current composed family/combined evidence -> one frozen push/one PR run -> full CI and source CLEAR -> READY -> auditor exact-tree GO -> guarded head-matched owner merge. Do not contaminate #2170 or its run, raise any budget, alter CPU sampling or publish source under this plan-only step.

## Frozen originals on actual core landing composition

Fe7 CARRY CLEAR on reviewed plan95146369771844bc67757a6035c7e167df60a03c3c93294e0476da6cf220b89d authorized originals. The original target is added on49a5b54f36232d1c926f4280141d92060db18e2b (actual repair core8f5f9a2f957 plus Hyperliquid originals). The Phemex runtime/Cargo and request inventory are unchanged from that base. No source adoption is claimed.

Executed cargo test --locked -p safe-cex-venue-phemex --test late_repair_context:
17 run, 4 intended ASSERTION RED, 13 GREEN, 0 ignored. No compile/helper failure.

| Intended RED | Genuine observed assertion |
| --- | --- |
| late_repair_waits_for_a_real_primary_preparation | Native emits unmanaged Action::Http before any genuine Primary Prepare. |
| held_repair_preparation_has_its_own_twenty_second_bound | Native has no repair preparation deadline at dispatch plus20000. |
| unclaimed_repair_ack_cannot_publish_or_finish_a_root | Native accepts the actual unclaimed request ID and changes Canceling to Canceled. |
| repair_admission_refusal_has_no_optimistic_cancel_or_io_and_preserves_peers | Real account commands exhaust registry credits; native late dispatch still changes Open to Canceling. |

The13 native GREENs pin empty/nonempty original cancel-all results, duplicate stream observations, cross-command same-side grouping, native10+1 bulk path, independent FIFO after Network loss, mixed ACK accepted/refused row effects, filled-order manual diagnostics, actual signing-expiry+5000 discovery with Primary, complete versus incomplete generation release, pre-snapshot reconciliation staying unloaded, and empty/one/many-row read preservation of loaded Store and stream rows. Clocks advance monotonically; fixture credentials are synthetic. Native and source release helpers use real LocalSigner plus host Prepare/Bind/Claim; no opaque owner/Context/entitlement/receipt is forged.

The suffix assertions after the four RED stops remain unexecuted until source. They are not additional original passes. Source-stage controls still to add: mint capacity result/journal isolation, typed provider/Internal refusal and physical timeout, stale completion/old-root peer isolation, exact other-symbol/pre-mint receipt retirement, revision-parity during concurrent stream updates, bound credit reuse, and direct native/JS parity. Core controls do not replace these venue integration gates.

Source must make all17 originals GREEN without changing their assertion bodies. Additional source-stage controls are additive and separately reviewed. Original execution logs and hashes stay outside the repository; no test output or profile artifact is committed.
