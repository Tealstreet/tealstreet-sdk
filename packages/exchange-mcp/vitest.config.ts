import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

export default defineConfig({
  plugins: [tsconfigPaths()],
  cacheDir: '../../.cache/vitest/exchange-mcp',
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
