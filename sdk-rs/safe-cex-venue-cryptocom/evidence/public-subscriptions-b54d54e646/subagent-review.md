# Read-only sub-agent review

Reviewer `/root/review_auth_witness` inspected committed SHA `b54d54e646fb15e46b644fd792c6cdbc7bceab08` against its actual direct parent `c4c1ec196d794b431bed97b2ede5eff17a41c6f5`, using committed views while mutation controls changed the working source. No Cargo, edits, CLAUDE.md/agent-memory reads or human review.

The earlier uncommitted ordinary-vector version had a real parity gap: index getters/custom iteration could throw or mutate array slots; replay callbacks could replace the Set. The committed revision exposes length/slice/map/iterator resolution-next at explicit host boundaries with original errors, uses associated opaque Slice/Mapped carriers, and stores replaceable Rc Set handles. The reviewer found those blockers resolved. Accessor grow/shrink/throw, inherited slice slots, custom iterator failure and retained old Set fixtures exercise them. The separate opaque-map case checks original native pointer identity.

No blocking production or metadata finding in the inspected committed bytes. Native state, frames, f64/UTF-16/slots, original errors and effects precede independent inventories. Floor68 ends case67 replay-replace-set; opaque-map floor1 ends index0. All new public functions/types/traits/methods and private shared helpers have explained exact-qualified mappings.

JavaScript host-operation adapters, class/constructor field binding, builtin lookup/JSON encoding and socket transport remain separate pending work. This review covers the function bodies at their declared boundaries, not end-to-end runtime adapters. Controls and clean gates were still running at review completion; the reviewer made no exit claim. Root measurements appear in gates.json and merge-gates.json.
