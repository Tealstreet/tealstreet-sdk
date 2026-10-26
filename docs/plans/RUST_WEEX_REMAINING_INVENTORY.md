# WEEX remaining inventory and identity custody

Owner: codex-uufub4. Reviewer: codex-hcmq3f. Source base:
`a71a90faf30e73132657e41df31dad7c9a07f1ca` on origin/feat/rust-rewrite.
The overseer assigned one venue PR with plan, originals and source. The earlier
classification plan `fae40a55571` was reviewed CLEAR; #2174 is now landed.

## Remaining inventory

| Inventory | File | Fingerprints / occurrences | Source meaning |
| --- | --- | ---: | --- |
| Request | history.rs | 2 / 2 | Typed request.kind data macros |
| Request | lib.rs | 4 / 4 | self.fail delegates to existing managed failure owners |
| Signing | account.rs | 1 / 1 | Unchanged CONFIG_INVALID diagnostic |
| Signing | secondary_tests.rs | 3 / 3 | Trusted provider fixtures and empty-secondary refusal |
| Credential | lib.rs | 1 term / 3 | Opaque Option<Identity> input and completeness validation |

These rows establish no missing raw endpoint migration. Every request,
signing, credential and socket manifest stays incoming-exact. Do not rename
helpers/options, hide macros, remove diagnostics or weaken financial oracles
to make conservative counts disappear. The two history macro rows remain
conservative under the existing checker; a parser-accuracy change is separate
shared gate work. Signing-module scan opt-in and opaque-option classification
also remain separate checker work, not venue exemptions.

## Concrete source change

Remove sign::Credentials, which duplicates both installed opaque identities
already held in Slots<Identity>. Build the slots once with exactly the current
primary/public-only and secondary/completeness filters. All native consumers
use those same immutable members through the shared declared role:

| Consumer | Existing eligibility and role |
| --- | --- |
| Account startup and refresh | Complete non-public Primary must exist |
| Private signed Upgrade | SessionAuth, always Primary |
| Identity | Primary eligibility, then ordinary SelectedRead |
| Ordinary history | Primary eligibility, ordinary SelectedRead prefers Secondary |
| Income history | Primary eligibility, PrimaryRead with History reason |
| Trading preflight and recipe | Mutation, always Primary |

The history root no longer constructs a second Slots from duplicate identities.
No endpoint, URL, payload, timer, deadline, refusal, outcome, snapshot or financial
mapping changes. The constructor's incomplete-primary deactivation and invalid
secondary notice stay exact. Empty private history remains empty without a
Primary even if a Secondary exists. Public-only setup still permits the already
supported secondary signed read routes; it does not activate private startup.

Delete only the obsolete identity container and its unused Identity import.
The shared URL encoder re-export remains. Existing secondary unit assertions
are adapted to pin literal expected installed refs and slot, rather than compare
against the deleted duplicate holder. Fixture inputs and all other assertions,
wire strings and clocks remain unchanged.

## Originals and qualification

Before source, execute the new native slot_custody target against unchanged
runtime. It checks actual managed history release with exact provider-derived
headers for Primary-only and Primary-plus-Secondary configurations, required
Primary income versus selected-read fills, one completion, duplicate DATA
inertness, and public-only private-history suppression. These characterize
existing behavior; no assertion RED is expected or required for this cleanup.
Record command/setup failures separately from executed controls.

After source, run the same controls, all WEEX native targets, strict all-target
clippy, fmt and full bans. Save the exact source patch and inventory-preservation
proof for hcm review. One reviewer CLEAR plus green correctness and WEEX replay
permits the guarded owner merge under venue speed mode. Do not wait for CPU or
certification; no auditor GO or local combined measurement is required while
combined headroom exceeds200KiB. The accepted latest shared measurement leaves
638,810 bytes, scoped to008c; it is not a fresh measurement of this venue slice.
CI remains the current family-size record. No Cargo, engine or host change is
part of this slice, and no signing/credential/request map zero is claimed.
