# Restore KuCoin Spot family throughput profile

Overseer recorded three unrelated timing misses after #1661: the earlier
+1.2% miss, landing #1673 (21a9081c18c/run39677179409, Rust9.1% faster versus
10% required), and Robinhood #1677 (0.185% short, retry passed). Aster stays
at one miss and is unchanged. One successful retry does not erase the tally.

Base: merged GRVT #1679 at bcf9fc37afe. Re-verify current landing and compare the pre-1661 build.sh arguments. KuCoin
Spot's venue crate already uses 3, but its WASM host, engine and dependencies
inherit the shared z overrides. Exempt only family kucoinspot from those shared
arguments, restoring the pre-1661 release3 settings in both --venue and
--families paths. Keep all other families, derivatives KuCoin's override,
combined/native profiles, wasm-opt and fixed budgets unchanged.

Use the existing public timing/memory gates as the actual regression evidence;
do not add a test that merely mirrors the shell condition. Compare complete
current-source baseline and candidate raw output sets (all identities retained,
metrics excluded only) with the strict ledger/report/declaration verifier and
independent timing-policy evaluation. Current1679CI39679489058 baseline has
2workloads/3modegroups/11stable Rust outputs (9public pairs), complete and green.
Refresh it if intervening relevant source changes make it non-equivalent.
No local release WASM build, timing sweep, comparator or manifest exclusions.

Commit this short plan first, implement the narrow build-policy change and
nearby size evidence, independent correctness/adversarial review, fresh full CI,
complete raw proof/public timing evidence and actual stripped bytes. Then fresh
separate exact-file revert guard, merge on green, verify actual parents/tree and
report to overseer. Browser cold-start measurement follows this repair.

## Pre-CI audit

Captured the real `family_args` arrays without invoking cargo: KuCoin Spot now
matches `01bfcbbb9d0f` exactly; all other 27 families match the current baseline
arguments byte for byte. Cargo.toml is unchanged from that pre-#1661 pin and
keeps release/host/engine defaults at 3. Shell syntax and diff checks pass.
The baseline CI package passes `validateCiWasm` on this fresh source base before
editing the build script (matching fingerprint and package hashes): stripped
wasm 677,234 B, SHA256
`991fbae3976573f187565dac536db7bf625c334d92f8f70ad6e8bffa65910e7e`.
Candidate bytes, complete 11-output comparison and timing evidence belong in
the PR before readiness/merge; baseline success alone does not certify it.
