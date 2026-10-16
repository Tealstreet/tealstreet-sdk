Input: a8b93f1aff50c58a5d8597828c2dc200a4dbf75c..25f8fdffbe9c823a74caec3083622973bcfa9204
Reviewer: native sub-agent review_spot_contracts
Verdict: static MERGE.
Renaming a real instrument declaration while retaining the actual mapping should trigger ASSIGNED-TARGET-RESOLVES before the independent target count. The parity crate has no Store dependency, so the selected test can reject the missing declaration without treating a Store compile failure as proof. No fake target, new exemption or weakened assertion. Other diff is constructor formatting.
Scope: committed objects only; no mutable source, Cargo, oracle or control execution. Runtime discrimination remained conditional on root's results.
