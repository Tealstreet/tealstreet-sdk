
## Actual local-custody composition

Actual #2040 landed as 5d825c7b965. The candle composition auto-merges every
source path, keeping the genuine caller root/physical PublicCors child and
#2040's initial local entry refusals together. Only README, CALLERS and the
request inventory need explicit resolution; the latter retains all incoming
prunes and removes only candle c5a021da count one and c873db6f count two.

The first local manifest-resolution script assumed a trailing comma on its last
key and failed; an unchecked shell continuation committed conflict markers in
91eb1cae4a7 before any push. Corrective commit 5a057c52fc4 restores valid JSON
with exactly the intended incoming-minus-candles delta. The ratchet's first
rerun also rejected the malformed immediate parent. This receipt retains the
failure and correction honestly without rewriting history; final checks run on
a head with valid immediate parents. The final published tree must match the
automatic source composition exactly and preserve every unowned incoming blob.

The auditor requires a fresh single PR Rust run on this exact composition,
even for the automatic shared lib.rs merge. No explicit dispatch or READY claim
until that run and its TxFlow family size gate pass.
