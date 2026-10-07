import { defineConfig } from 'vitest/config';

// The drift test resolves `@tealstreet/script-api-core` through this
// package's tsconfig alias, the same one `tsc` reads. `safe-cex` is aliased
// too because the SDK's capability types import `ExchangeName` from it — a
// type-only import, so nothing of safe-cex is ever loaded at runtime.
export default defineConfig({
  resolve: { tsconfigPaths: true },
  test: { globals: true },
});
