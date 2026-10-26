import tsconfigPaths from 'vite-tsconfig-paths';
import { defineConfig } from 'vitest/config';

const wasmTests = process.env.SAFE_CEX_WASM_TESTS === '1';

export default defineConfig({
  plugins: [tsconfigPaths()],
  test: {
    environment: 'node',
    include: wasmTests ? ['src/**/*.wasm.test.ts'] : ['src/**/*.test.ts'],
    exclude: ['**/node_modules/**', ...(wasmTests ? [] : ['**/*.wasm.test.ts'])],
  },
});
