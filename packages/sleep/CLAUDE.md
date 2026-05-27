# CLAUDE.md — @tealstreet/sleep

Promise-based async delay utility.

- **Key file**: `sleep.ts` — `sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms))`
- **Build**: `yarn build-force` (tsup)
- **Note**: Zero dependencies. Single-line utility used throughout the codebase.
