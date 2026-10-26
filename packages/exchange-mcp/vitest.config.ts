import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tsconfigPaths()],
  cacheDir: '../../.cache/vitest/exchange-mcp',
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    exclude: process.env.SAFE_CEX_WASM_TESTS ? [] : ['**/*.wasm.test.ts'],
  },
});
