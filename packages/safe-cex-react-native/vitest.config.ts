import { defineConfig } from 'vitest/config';
import tsconfigPaths from 'vite-tsconfig-paths';

export default defineConfig({
  plugins: [tsconfigPaths()],
  test: {
    globals: true,
    environment: 'node',
    exclude: process.env.SAFE_CEX_WASM_TESTS ? [] : ['**/*.wasm.test.ts'],
    // This package is a thin runtime adapter and has no tests yet. Without
    // this, `vitest run` exits 1 purely for finding none, which fails the
    // whole repo test task for something that is not a failure. Matches how
    // ~20 other packages here handle it.
    passWithNoTests: true,
  },
});
