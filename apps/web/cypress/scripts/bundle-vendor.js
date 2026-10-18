#!/usr/bin/env node

/**
 * Bundles workspace package dependencies used by Cypress tests into a single
 * self-contained vendor file. This eliminates the need to install the full
 * monorepo just to run E2E tests.
 *
 * Usage: node apps/web/cypress/scripts/bundle-vendor.js
 *
 * Output: apps/web/cypress/support/vendor.js
 *
 * Run this whenever source packages change (@tealstreet/math, @tealstreet/safe-cex,
 * @tealstreet/proxy-ips-lists) or as a CI step before Cypress.
 */

const path = require('path');

const ROOT = path.resolve(__dirname, '../../../..');
const ENTRY = path.join(__dirname, '../support/vendor-entry.ts');
const OUTPUT = path.join(__dirname, '../support/vendor.js');

async function main() {
  // Dynamically require esbuild (works with both local install and npx)
  const esbuild = require('esbuild');

  await esbuild.build({
    entryPoints: [ENTRY],
    bundle: true,
    outfile: OUTPUT,
    format: 'cjs',
    platform: 'node',
    target: 'es2020',
    logLevel: 'info',
    // Map @tealstreet/* imports to their actual paths in the monorepo.
    // This lets esbuild resolve workspace packages without yarn install.
    alias: {
      '@tealstreet/math': path.join(ROOT, 'packages/math'),
      '@tealstreet/safe-cex': path.join(ROOT, 'packages/safe-cex'),
      '@tealstreet/proxy-ips-lists': path.join(ROOT, 'packages/proxy-ips-lists'),
    },
  });

  console.log(`Vendor bundle written to: ${path.relative(ROOT, OUTPUT)}`);
}

main().catch((err) => {
  console.error('Failed to bundle vendor dependencies:', err.message);
  process.exit(1);
});
