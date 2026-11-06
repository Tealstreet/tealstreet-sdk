# Hyperliquid msgpack without rmp_serde

**Change.** The Hyperliquid wasm family no longer links rmp_serde.
- `sign::statement::msgpack(json)` re-encodes an action's JSON as msgpack. The
  JSON keeps field order, and the writer uses rmp_serde's smallest headers.
- Dispatch (`sign::dispatch::Exchange::new`) and remote-statement derivation
  both use it.
- The statement's fields are read from one Node parse instead of a second
  serde-derived struct. They keep the same strictness: unknown, duplicate,
  missing and mistyped fields are refused.

**Pins.**
- The writer equals rmp_serde at every header boundary.
- Dispatch msgpack equals rmp_serde for every HL action shape, including the
  official vectors.
- The field reader matches the old derive over good and malformed statements.

**Callers.**
- `sign::action_hash` and `sign::sign` keep rmp_serde for TxFlow and tests.
- NX: none. Wire: unchanged.

**Obligation.** A new HL wire field must be a string, bool, unsigned integer,
option or sequence. A float, bytes or non-string map key would encode
differently through JSON. The parity test fails if one is added to
`remote_actions()`.

**Deletion prerequisites.** None.
