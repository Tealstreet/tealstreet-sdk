# V3 asset and futures read scope

The unused strict account-info decoder, account-balance mapper and their DTOs
are retired after production caller checks. Their private account-mode enum,
account-type deserializer, facade cases and obsolete census/receipt are retired
too. The configured legacy WOO class does not import this V3 module.

The remaining fixture retains every helper, asset, futures, envelope, metadata
and pagination case and native field assertion. Its five surviving declaration
records still inspect numeric bits and map-only refusal before wire projection.
Only the removed account decoder’s 16 cases, balance mapper’s eight cases and
account declaration/enum assertions are dropped; they are not retained coverage.

The five surviving compiled controls remain in the scoped script; only the
orphaned account-decoder control is removed. The narrowed script uses the SDK
working directory and one Cargo job. No new control execution is claimed here.

FIX_AUDIT_LEDGER.md rows 8 and 52 record the retirement. Constructed native and
maintained-source boundary probes pass before and after; the actual legacy
validateAccount/fetchBalance HTTP path remains available. No production TS,
legacy balance writer or shared model type is changed.
