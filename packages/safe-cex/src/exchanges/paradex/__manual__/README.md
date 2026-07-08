# paradex/**manual**

Ad-hoc `test-*.{ts,js}` scripts used to poke the Paradex adapter by hand
(balance dumps, leverage set, ws probes). **Not** vitest specs — they are
`test-*` prefixed, not `*.test.*`, so the runner never picks them up, and this
directory is excluded from `packages/safe-cex/tsconfig.json` so `yarn typecheck`
does not sweep them.

Run one directly with `vite-node`/`tsx` when debugging, e.g.
`npx tsx src/exchanges/paradex/__manual__/test-dump-balance.ts`. They read local
`.apikey.json` and hit live Paradex — never wire them into CI.
