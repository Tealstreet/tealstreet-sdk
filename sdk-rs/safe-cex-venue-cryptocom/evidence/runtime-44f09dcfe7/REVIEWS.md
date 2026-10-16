Static adversarial reviews pinned to `44f09dcfe78686107640a9d94147e2d527f8b95e`, each in a separate detached checkout.

Body reviewer: **MERGE for body semantics**. Found no mismatch in startup, polling, factory/interceptor or constructor ordering. Verified all 23 mapped Rust targets exist. Requested exact qualified names in row notes. `RuntimeOps::existing_member` remains a class bridge FINDING. Native prototype hooks retain callbacks but do not independently model descriptor flags; source descriptor assertions provide no native descriptor proof.

Witness reviewer: **MERGE, bounded to the witnessed bodies**. Found missing original-receiver assertions at `tests/runtime.rs` interceptor `use` and public/private socket dependency branches. The constructor helper directly called `port::create_api` instead of invoking the supplied API callable. These are witness gaps; no corresponding production body defect was found.

Both reviewers performed static source/diff inspection only. Neither ran Cargo, tests, controls, isolation, clippy, extractor or CI. Runtime source, three witnesses and two manifests were byte-unchanged across the integration merge into this revision. Subsequent review fixes and their measurements have separate revision receipts.
