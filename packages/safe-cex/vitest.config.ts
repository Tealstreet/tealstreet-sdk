import react from '@vitejs/plugin-react';
import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

// `yarn test-wasm` runs only the tests that drive the built wasm engine.
const wasmTests = process.env.SAFE_CEX_WASM_TESTS === '1';
const testFiles = wasmTests ? ['./src/**/*.wasm.test.{js,jsx,ts,tsx}'] : ['./src/**/*.test.{js,jsx,ts,tsx}'];

export default defineConfig({
  plugins: [
    tsconfigPaths(),
    react({
      // fastRefresh: false,
    }),
  ],
  test: {
    globals: true,
    environment: 'happy-dom',
    setupFiles: './config/test/setupVitest.ts',
    passWithNoTests: true,
    /*
    deps: {
      experimentalOptimizer: {
        enabled: true,
      },
    }, */
    cache: {
      dir: '../../.cache/vitest/core-lib',
    },
    coverage: {
      provider: 'v8',
      reporter: ['text', 'clover'],
      include: ['src/**/*.{js,jsx,ts,tsx}'],
      // @ts-expect-error - 'all' is a valid option but types don't include it
      all: true,
    },
    // To mimic Jest behaviour regarding mocks.
    // @link https://vitest.dev/config/#clearmocks
    clearMocks: true,
    mockReset: true,
    restoreMocks: true,
    include: testFiles,
    exclude: [
      '**/node_modules/**',
      '**/dist/**',
      '**/.next/**',
      '**/.{idea,git,cache,output,temp}/**',
      '**/*.integration.test.{js,jsx,ts,tsx}',
      '**/*.network.test.{js,jsx,ts,tsx}',
      // Need the built wasm engine; the Rust CI job runs them (yarn test-wasm).
      ...(wasmTests ? [] : ['**/*.wasm.test.{js,jsx,ts,tsx}']),
    ],
  },
});
