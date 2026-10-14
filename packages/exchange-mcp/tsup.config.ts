import { defineConfig } from 'tsup';

// One self-contained file, so `npx @tealstreet/exchange-mcp` installs nothing else.
export default defineConfig({
  entry: ['src/index.ts'],
  format: ['esm'],
  platform: 'node',
  target: 'node20',
  noExternal: [/.*/],
  splitting: false,
  clean: true,
  banner: {
    js: "#!/usr/bin/env node\nimport { createRequire } from 'node:module'; const require = createRequire(import.meta.url);",
  },
});
