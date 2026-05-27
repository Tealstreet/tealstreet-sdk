import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

const testFiles = ['./src/**/*.test.{js,jsx,ts,tsx}'];

export default defineConfig({
  plugins: [tsconfigPaths()],
  test: {
    globals: true,
    environment: 'node',
    passWithNoTests: true,
    cache: {
      dir: '../../.cache/vitest/poseidon-crypto',
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'clover'],
    },
    clearMocks: true,
    mockReset: true,
    restoreMocks: true,
    include: testFiles,
    exclude: ['**/node_modules/**', '**/dist/**', '**/.next/**', '**/.{idea,git,cache,output,temp}/**'],
  },
});
