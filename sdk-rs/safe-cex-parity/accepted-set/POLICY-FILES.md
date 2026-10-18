# Reviewed policy storage

Reviewed records live in `accepted-set/<table>/<crate>.json`. For example,
transport Clone reviews belong in `construction/safe-cex-transport.json`; GRVT
fixed alias identities belong in `generic-aliases/safe-cex-venue-grvt.json`.
Keep each record's existing compiler owner or source-helper key unchanged.

The tables are `construction`, `default-traits`, `receiver-traits`,
`generic-aliases`, `function-pointer-aliases`, `constant-outputs`,
`local-functions` and `local-type-helpers`. The separate finite
`inherited-constant-outputs` table uses the same layout for individually reviewed
constant method witnesses. Contracts already use the independent
`contracts/<crate>.json` layout and `loadContracts()`.

JavaScript readers import `loadPolicy` from `scripts/accepted-policy-files.mjs`
and call, for example, `loadPolicy('construction')`. An isolated fixture may pass
its accepted-set root as the second argument. The loader merges every shard,
requires each key to belong to its filename's crate, and refuses missing/empty
tables, malformed shards, duplicate owners, unexpected files and symlinks. It
also refuses a restored `<table>.json`; there is no flat-file fallback.

When merging master into an existing review branch, preserve the master shards
and port only the branch's genuine new or changed records into their crate
files. Never restore the deleted flat maps. Native mutation controls edit the
specific crate shard and restore its original bytes in `finally`.

This is storage only. Existing compiler signatures, alias identities, source
pins, Clone/Copy traits, role reviews and native observations remain required.
Evidence ledgers, fixed compiler fixtures and control registries retain their
own formats; they are not maps of independently appended crate reviews.

The callable-alias family admits one or two distinct, unbounded type parameters
on an actual compiler-authenticated `Rc<dyn Fn(...) -> ...>` target. Every type
parameter is fixed to `serde_json::Value` by the probe, independently of the
contract. Bounds, lifetimes, higher-ranked binders and additional parameters
remain unsupported. Exact raw/expanded targets and explicit non-decoder reviews
are required. These native trait receipts classify the fixed alias type; actual
production-binding callback witnesses separately establish callback behavior.

The initial cutover from master `d9d338ebff` preserved all 160 records across
eight tables in 52 shards. The cutover compares every complete parsed old map
with `loadPolicy`, including native adapter strings and negative zero, using:

```sh
ACCEPTED_LEGACY_POLICY_DIRECTORY=/path/to/original-tables \
  node --test sdk-rs/safe-cex-parity/scripts/accepted-policy-files.test.mjs
```

The permanent fixture covers both compiler and source keys, Unicode, Rust
adapter text and negative zero. Executed controls remove crate ownership,
drop a merged record and permit a restored flat file; each must fail its named
assertion and pass after restoration.

Receiver classifications use `itemSha256`, with no whole-file `sourceSha256`
fallback. For an unsupported receiver, hash the complete compiler impl item text;
for an uninstantiated default, hash its compiler method item text. Keep `file`,
`category` and the individual reviewed `reason`. Compute the digest with
`receiverItemSha256(span, source)` from `accepted-inventory.mjs`, using the fresh
ordinary rustdoc item's span and exact current source bytes. The gate obtains
that span from the compiler itself; policy cannot supply or redirect it.

Coordinates are one-based Unicode scalar columns with an exclusive end. They
are resolved anew and are not stored in policy: an unrelated edit before or
after the item may move its coordinates without invalidating the review. Any
change inside the item, including whitespace, invalidates its pin. Dependencies
called by the item retain their separate inventory and behavioral obligations.
This does not retire role review or native evidence when a reviewed body changes.

The receiver item cutover rehashed all 76 existing rows in seven crate shards;
owner keys, categories, source files and reasons were preserved. The compiler
regression builds an original impl, an unrelated prefix/suffix edit and an impl
body edit, proving pass/pass/fail and exact restoration. It also rejects the old
whole-file schema, invalid coordinates and hash-check removal.
