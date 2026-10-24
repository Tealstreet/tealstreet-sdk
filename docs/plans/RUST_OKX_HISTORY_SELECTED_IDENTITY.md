# OKX history and closed-position selected identity

Plan first, codex-ih35js, 2026-10-24. Actual landing base
`db6fa45bbb7cbbfdfeb0aa40d3dd93caac9c9e0b`.

## Scope and inventory

Remove the two selection sites introduced by the managed history conversions:

| Path | Existing counted site | Planned count |
| --- | --- | --- |
| venues/okx/src/closed.rs | read_credentials: 1, local Slots construction | remove file entry |
| venues/okx/src/history.rs | read_credentials: 2, including send-time slot-to-identity remap | read_credentials: 1 |

The remaining history presence gate prevents secondary account snapshots from
establishing a primary client-order lookup identity. Keep it unchanged. This is
a credential-selection inventory reduction, not a request-inventory reduction.
No checker exemptions, renamed credential terms or enlarged inventory.

#2018 already landed as `3573b9212e5` under its valid GO. The subsequent HOLD
arrived after merge completion. Per the overseer's no-revert ruling, #1831
records closed:1 and history:2 in combo-5b; this follow-up removes those two
counts only after genuinely integrating the actual combo-5b landing.

## Ownership and native policy

#1991 owns account.rs and #1996 owns discovery in lib.rs. Both add local Slots
construction using the default policy: a secondary key can serve a read without
a primary. Okx::new filters the primary for public_data_only and validity, but
does not filter read_credentials. Closed positions retain their primary-required
gate and SlotPolicy { secondary_requires_primary: true }. History's ordinary
private remote reads retain their existing primary-required gate, while lookup
uses Primary with ClientOrderLookup and public trades stay unsigned.

Wait for #1991/#1996 actual landing before shared lib.rs edits. Coordinate the
narrow common Slots construction with claude-4yjlw0 and the inventory with
claude-m26ps4. Prefer the owner's proposed read_slots(policy) declaration helper:
it constructs engine Slots from the existing filtered primary and supplied read
identity, and every caller declares its native policy. It must not implement key
choice, admission, retry, expiry or signing. Preserve the still-live
read_credential API and callers; if its construction moves into this helper,
preserve its default policy and signature, with no increase in lib.rs counts.
Do not impose closed-position policy on account/discovery or move a counted
legacy selector into an uncounted alias.

## Shared selection and immutable history custody

Closed already has a SelectedRead command root and queues each ReadLeg with the
identity supplied by the shared selection. Replace its local Slots construction
with the coordinated common declaration; keep all root/leg/history behavior.

For history, capture the SelectedRead at command entry and retain its ReadRoot
through every physical page. Use ReadLeg::queue's identity argument directly for
the existing sign::Http dispatch recipe. Remove the local CredentialSlot match
and lookup of self.options.read_credentials/self.credentials from send_history.
Never reselect from current venue options at a later page or infer identity from
a Context slot. Mint 20 s/Never children from the original caller receipt root
only at the eligible native 100 ms dequeue. Stream the same native data field.

Public trades use the existing unsigned caller Read root and managed request
path, retaining demo headers. Missing selected identity must preserve existing
local terminal ordering and shapes, including cached/unknown lookup outcomes;
selection failure is not permission to add an earlier refusal. Local terminals
still require genuine entry admission before any effect. Maintain exact
ReadCompletion delivery/terminal authority, strict root/local refusal and
provider refusal for ordinary history, native soft physical query failures,
accepted-output-based closed-position recovery, exact-owner retirement and
inert stale DATA/timers. Preserve raw mappers, queries, page caps and pacing.

## RED and verification sequence

1. Commit this plan before source changes. Obtain bounded API/policy review.
2. After actual combo-5b and the coordinated lib owners land, merge them plainly.
   Save the actual existing checker baseline. Regenerate the credential inventory
   with CREDENTIAL_INVENTORY_WRITE=1 only after adoption. First commit a removal-
   only inventory RED against unchanged production: expected closed absent and
   history count 1 must fail the real checker because source still has 1 and 2.
   Keep every incoming inventory entry exact. Do not fabricate a source-text
   unit test or a missing-method compile error as the RED.
3. Add real native controls before implementation: both keys, primary only,
   secondary only, public_data_only/filtered primary, ordinary history versus
   primary lookup, optional public trades/demo header, provider refusal and
   later-page selection/signature/egress. Existing root deadline, delayed child,
   reuse/late DATA, cap and financial controls remain unchanged. Record working
   controls separately from the inventory RED; this is behavior preservation.
4. Implement direct shared selection consumption, then regenerate only the exact
   own credential-key shrink. Run scoped OKX tests and strict Clippy sequentially
   with CARGO_BUILD_JOBS=4, plus full bans and both inventory check/test pairs.
   CI covers workspace and WASM/replay/CPU gates; no local broad build.
5. Independent source/adversarial, caller/policy and map reviews; push a small
   draft PR, fresh CI on actual 5b, freeze and send SHA/run READY. No merge
   without the overseer's explicit window.

Architecture gate: **No**, provided the implementation only consumes the shared
Slots/SelectedRead/ReadRoot/ReadLeg algorithms. A helper is construction data,
never a venue-local replacement for engine selection. No intentional caller-
visible behavior change; any discovered policy difference requires a new ruling.
