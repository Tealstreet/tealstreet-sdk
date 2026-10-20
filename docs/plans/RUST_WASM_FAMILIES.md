# Per-family WASM loading

Branch `feat/rust-wasm-families` from `feat/rust-rewrite` at `ed2128a337`.
This follows #1344's closing note: re-port only the per-venue-family packaging
and loading, without batched delivery, and measure it on its own.

## Baseline

The browser, Worker and Node default loaders (`@tealstreet/safe-cex-wasm/engine`)
load the combined engine: every venue, about 12 MB per target. CI replay gates
already build one venue (`build.sh --venue`), so gate numbers describe family
builds. The CLI, React Native and the MCP bundle embed the combined engine on
purpose, since they serve many venues from one artifact.

## Plan

1. Measure: `yarn rust:measure-family` runs one venue's family build against the
   combined build in alternating pairs, recording web/Node bytes (raw, gzip,
   brotli), cold Node load (wall, CPU, RSS), and public replay main/steady/process
   CPU, retained memory and RSS. Outputs must be identical. Run it for all 30
   public gate venues on CI with 7 pairs.
2. Decide: ship only on a clear byte and cold-load win with no CPU regression
   (family median ≤ combined median plus noise on main and steady for every
   venue). Otherwise close with the numbers.
3. If shipping: build per-family packages (`pkg/{web,node}/<family>`), add family
   exports, and have the default web/Node loaders load the venue's family with
   per-family initialization caching. Keep the combined exports for the CLI,
   React Native and MCP.
