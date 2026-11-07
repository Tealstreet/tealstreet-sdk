#!/usr/bin/env node

/**
 * Wrapper script to run Cypress E2E tests with cleaner flag syntax
 *
 * Usage:
 *   yarn test:e2e --exchange woo                          (runs against localhost:3000)
 *   yarn test:e2e --exchange woo --url preview             (runs against preview.tealstreet.io)
 *   yarn test:e2e --exchange woo --url v3                  (runs against v3.tealstreet.io)
 *   yarn test:e2e --exchange woo --url dev                 (runs against dev.tealstreet.io)
 *   yarn test:e2e --exchange woo --url https://custom.url  (runs against any URL)
 *   yarn test:e2e --exchange woo --tests reduce-orders
 *   yarn test:e2e --open --browser chrome --exchange woo
 *   yarn test:e2e --headed --browser chrome --exchange woo
 *   yarn test:e2e --exchange woo --record                  (record to Cypress Cloud)
 */

const { spawn } = require('child_process');
const path = require('path');

// Valid test names (must match the test names in spec.cy.ts)
const VALID_TESTS = [
  'all',
  'load-balance',
  'place-market-orders',
  'place-limit-orders',
  'position-flip',
  'partial-closes',
  'position-mode',
  'reduce-orders',
  'hedge-mode',
  'bracket-orders',
  'margin-slider',
  'stop-orders',
  'bad-api-key',
  'add-api-key',
];

// Read valid exchanges from exchanges.js (single source of truth)
const exchangeConfigs = require('../config/exchanges.js');
const { VALID_EXCHANGES } = require('../config/exchangeLists.js');

// Named environment shortcuts for --url flag
const URL_ALIASES = {
  preview: 'https://preview.tealstreet.io',
  v3: 'https://v3.tealstreet.io',
  dev: 'https://dev.tealstreet.io',
};

function resolveUrl(input) {
  if (!input) return '';
  if (URL_ALIASES[input]) return URL_ALIASES[input];
  return input; // Assume it's a full URL
}

// Parse command line arguments
const args = process.argv.slice(2);

// Handle --list flag
if (args.includes('--list') || args.includes('--list-tests')) {
  console.log('\n📋 Available E2E Tests\n');
  console.log('Run all tests:');
  console.log('  all\n');
  console.log('Individual tests:');
  VALID_TESTS.filter((t) => t !== 'all').forEach((test) => {
    console.log(`  ${test}`);
  });
  console.log('\nExamples:');
  console.log('  yarn test:e2e --exchange woo --tests all');
  console.log('  yarn test:e2e --exchange woo --tests position-mode');
  console.log('  yarn test:e2e --exchange woo --tests load-balance,place-market-orders');
  console.log('  yarn test:e2e --exchange woo --url preview');
  console.log('  yarn test:e2e --exchange woo --url v3');
  console.log(`\nURL shortcuts: ${Object.keys(URL_ALIASES).join(', ')} (or any full URL)\n`);
  process.exit(0);
}

const env = { ...process.env };
let openMode = false;
let browser = 'chrome';
let headed = false;
let baseUrl = '';
let record = false;

// Parse flags
for (let i = 0; i < args.length; i++) {
  const arg = args[i];

  if (arg === '--exchange' && i + 1 < args.length) {
    env.CYPRESS_EXCHANGE = args[i + 1];
    i++;
  } else if (arg === '--tests' && i + 1 < args.length) {
    env.CYPRESS_ENABLED_TESTS = args[i + 1];
    i++;
  } else if (arg === '--open') {
    openMode = true;
  } else if (arg === '--browser' && i + 1 < args.length) {
    browser = args[i + 1];
    i++;
  } else if (arg === '--headed') {
    headed = true;
  } else if (arg === '--url' && i + 1 < args.length) {
    baseUrl = args[i + 1];
    i++;
  } else if (arg === '--record') {
    record = true;
  }
}

// Validate exchange
if (env.CYPRESS_EXCHANGE && !VALID_EXCHANGES.includes(env.CYPRESS_EXCHANGE)) {
  console.error(`\n❌ Error: Invalid exchange "${env.CYPRESS_EXCHANGE}"\n`);
  console.error('Valid exchanges:');
  VALID_EXCHANGES.forEach((ex) => console.error(`  - ${ex}`));
  console.error('');
  process.exit(1);
}

// Validate tests
if (env.CYPRESS_ENABLED_TESTS) {
  const requestedTests = env.CYPRESS_ENABLED_TESTS.split(',').map((t) => t.trim());
  const invalidTests = requestedTests.filter((test) => !VALID_TESTS.includes(test));

  if (invalidTests.length > 0) {
    console.error(`\n❌ Error: Invalid test name(s): ${invalidTests.join(', ')}\n`);
    console.error('Valid test names:');
    VALID_TESTS.forEach((test) => console.error(`  - ${test}`));
    console.error('');
    process.exit(1);
  }
}

// Build Cypress command arguments
const cypressArgs = openMode
  ? ['open', '--e2e', '--browser', browser]
  : [
      'run',
      '--spec',
      'cypress/e2e/spec.cy.ts',
      '--browser',
      browser,
      ...(headed ? ['--headed'] : []),
      ...(record ? ['--record'] : []),
    ];

// Resolve URL alias and add baseUrl override if provided
const resolvedUrl = resolveUrl(baseUrl);
if (resolvedUrl) {
  cypressArgs.push('--config', `baseUrl=${resolvedUrl}`);
}

// Check if requested tests are skipped
let skippedRequestedTests = [];
if (env.CYPRESS_EXCHANGE && env.CYPRESS_ENABLED_TESTS && env.CYPRESS_ENABLED_TESTS !== 'all') {
  const config = exchangeConfigs[env.CYPRESS_EXCHANGE];
  if (config && config.skipTests) {
    const requestedTests = env.CYPRESS_ENABLED_TESTS.split(',').map((t) => t.trim());
    skippedRequestedTests = requestedTests.filter((test) => config.skipTests.includes(test));
  }
}

// Log configuration
console.log('\n🧪 Running Cypress E2E Tests');
console.log(`   Mode: ${openMode ? 'Interactive (UI)' : headed ? 'Headed (Visible Browser)' : 'Headless (CI)'}`);
if (openMode || headed) {
  console.log(`   Browser: ${browser}`);
}
console.log(`   Exchange: ${env.CYPRESS_EXCHANGE || 'okx (default)'}`);
console.log(`   Tests: ${env.CYPRESS_ENABLED_TESTS || 'all'}`);
if (resolvedUrl) {
  console.log(`   URL: ${resolvedUrl}`);
} else {
  console.log(`   URL: http://localhost:3000 (default)`);
}
if (record) {
  console.log(`   Recording: Cypress Cloud`);
}

if (skippedRequestedTests.length > 0) {
  console.log(`\n⚠️  WARNING: The following requested tests are in the skipTests configuration:`);
  skippedRequestedTests.forEach((test) => {
    console.log(`   - ${test}`);
  });
  console.log(`\n   These tests will NOT run. To enable them, edit:`);
  console.log(`   cypress/config/exchanges.ts (${env.CYPRESS_EXCHANGE}.skipTests)\n`);
}
console.log('');

// Run Cypress
const cypress = spawn('cypress', cypressArgs, {
  env,
  stdio: 'inherit',
  cwd: path.join(__dirname, '../..'),
});

cypress.on('exit', (code) => {
  process.exit(code);
});
