# WOO cardinality audit

## Scope and evidence

This is an open audit, not a coverage verdict. Inspection found 17 zip sites:
15 had immediate length guards and two public-writer comparisons did not.
Length equality alone does not establish a nonempty corpus, fixed subcorpus
coverage, or an independent number of comparisons.

The audit covers truncating pairs, empty production, both sides empty,
oracle-derived loop bounds, extra native output, disappearing subcorpora,
and consuming decoder mutations. A decoder mutation must consume its input
before dropping output; a trailing-element rejection is not assertion coverage.

## Local guards

The public-writer target pins 87 source cases, 174 source ticker rows, two
source tickers per case and two native tickers independently. Both field walks
use the fixed range 0..2. Identity capture totals are 250 legacy cases,
312 contract cases and six public records. Mapper captures pin 215 cases and
28/56/13/26 compared market/stats/book/candle cases respectively.

Additional guards pin the original corpus totals for fills, journal, utilities,
pricing, responses, history, public mapping and legacy declarations. Alias
witnesses pin two requests, one market, and one order plus one clock; numeric
book witnesses pin one level on each side and the metadata-only record pins
zero rows. These are authored assertions, not completed mutation proofs.

Original fixture artifacts have not been edited. The guards do not change
historical capture bytes or make a retroactive independence claim.

## Measured checks and invalid controls

The first public-writer guard version passed its scoped target: one test,
zero failures. This run preceded replacement of its zip loops with fixed
index walks and does not validate that later change.

Two attempted production writer mutations cleared or shortened the Store via
an additional update. Both compiled and failed at `writer notification order`
because the extra update notified subscribers. Neither is a valid cardinality
control, and neither establishes that the original zip assertions were sound.

## Still required

- Pin acceptance-filtered identity subsets per record and independent mapper
  output cardinalities per case.
- Pin runtime targets, cancel-after subcorpora and the HOW parser corpus.
- Run empty, subset, extra-output and source-guard-bypass controls against each
  comparison target, recording compilation and the exact failing assertion.
- Run consuming collection-decoder controls and retain the mutant while
  bypassing the new guards to establish which assertions detect the loss.
- Count executed native comparisons independently of source loop bounds.

The inspected-site census and the executed-control ledger remain separate.
No whole-venue cardinality coverage is claimed.
