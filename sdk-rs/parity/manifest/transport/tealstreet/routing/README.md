# Declarative routing coverage

`routingTable.ts.toml` intentionally has zero callable-symbol rows. The extractor
collects callable declarations and types, not this module’s declarative data;
`parity:extract --check` makes no symbol-coverage claim for `routingTable.ts`.

`REST_JP`, `HOW_REGION_CONDITIONAL`, and the `WS` host literals are covered by
`sdk-rs/safe-cex-transport/scripts/routing.mjs`. Root `yarn routing:check` checks
all 47 exchanges × 24 flag/region/site combinations against the golden snapshot,
then checks generated Rust freshness and live cross-language equivalence. HOW
tokens come from the owned egress source. The golden matrix includes Brazil
cases for every exchange, including venues where Brazil is a no-op.

This annotation lives beside the TOML because the extractor canonicalizes TOML,
strips comments, and permits only `ts` in `[file]`. Adding fake symbol rows or
an unrecognized file annotation would violate the manifest contract. For routing
behavior, see [the shared guidance](../../../../../../packages/safe-cex/CLAUDE.md#proxy--tunnel-architecture).
