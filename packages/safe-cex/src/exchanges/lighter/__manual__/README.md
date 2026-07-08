# lighter/**manual**

Ad-hoc `test-*.ts` scripts used to poke the Lighter adapter by hand. **Not**
vitest specs — they are `test-*` prefixed, not `*.test.*`, so the runner never
picks them up, and this directory is excluded from
`packages/safe-cex/tsconfig.json` so `yarn typecheck` does not sweep them.

Run one directly with `vite-node`/`tsx` when debugging. They read local
credentials and hit live Lighter — never wire them into CI.
