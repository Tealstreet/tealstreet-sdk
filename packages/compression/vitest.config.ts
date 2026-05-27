import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

const testFiles = ['./src/**/*.test.{js,ts}'];

export default defineConfig({
  plugins: [tsconfigPaths()],
  test: {
    globals: true,
    environment: 'happy-dom',
    passWithNoTests: true,
    cache: {
      dir: '../../.cache/vitest/compression',
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'clover'],
    },
    clearMocks: true,
    mockReset: true,
    restoreMocks: true,
    include: testFiles,
    exclude: ['**/node_modules/**', '**/dist/**'],
  },
});
