import { defineConfig } from 'vitest/config';

import config from '../../vitest.config';

export default defineConfig({
  ...config,
  test: {
    ...config.test,
    include: ['./src/__tests__/audit-regressions/rust.kraken-hosted-nonce.originals.ts'],
    fileParallelism: false,
    maxWorkers: 1,
  },
});
