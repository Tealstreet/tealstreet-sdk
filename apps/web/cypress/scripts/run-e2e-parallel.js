#!/usr/bin/env node

/**
 * Parallel E2E test runner for multiple exchanges
 *
 * Usage:
 *   yarn test:e2e:parallel --exchanges okx,woo,blofin
 *   yarn test:e2e:parallel --exchanges all
 *   yarn test:e2e:parallel --exchanges okx,woo --tests reduce-orders
 *   yarn test:e2e:parallel --exchanges all --browser chrome
 *   yarn test:e2e:parallel --help
 */

const { spawn } = require('child_process');
const path = require('path');
const fs = require('fs');
const readline = require('readline');

// ANSI color codes
const colors = {
  reset: '\x1b[0m',
  bright: '\x1b[1m',
  dim: '\x1b[2m',
  green: '\x1b[32m',
  yellow: '\x1b[33m',
  blue: '\x1b[34m',
  cyan: '\x1b[36m',
  red: '\x1b[31m',
  magenta: '\x1b[35m',
};

// Exchange-specific colors (cycle through for variety)
const exchangeColors = [colors.cyan, colors.blue, colors.magenta, colors.yellow];
const exchangeColorMap = {};

function getExchangeColor(exchange) {
  if (!exchangeColorMap[exchange]) {
    const colorIndex = Object.keys(exchangeColorMap).length % exchangeColors.length;
    exchangeColorMap[exchange] = exchangeColors[colorIndex];
  }
  return exchangeColorMap[exchange];
}

function formatExchangeTag(exchange) {
  const color = getExchangeColor(exchange);
  return `${color}${colors.bright}[${exchange.toUpperCase()}]${colors.reset}`;
}

function colorizeOutput(line, exchange) {
  // Color checkmarks green
  if (line.includes('✓')) {
    line = line.replace(/✓/g, `${colors.green}✓${colors.reset}`);
  }

  // Color X marks red
  if (line.includes('✖') || line.match(/^\s+\d+\)/)) {
    line = `${colors.red}${line}${colors.reset}`;
  }

  // Color "passing" green
  if (line.includes('passing')) {
    line = line.replace(/(\d+) passing/, `${colors.green}$1 passing${colors.reset}`);
  }

  // Color "failing" red
  if (line.includes('failing')) {
    line = line.replace(/(\d+) failing/, `${colors.red}$1 failing${colors.reset}`);
  }

  // Dim separator lines
  if (line.match(/^[─┌┐└┘│├┤┬┴┼═]+/) || line.includes('─────')) {
    line = `${colors.dim}${line}${colors.reset}`;
  }

  // Color test names (lines starting with whitespace and text)
  if (line.match(/^\s{4,}(Test|Place|Load|Limit)/)) {
    line = `${colors.bright}${line}${colors.reset}`;
  }

  return line;
}

// Read valid exchanges from exchanges.js (single source of truth)
const { VALID_EXCHANGES, DEFAULT_EXCHANGES } = require('../config/exchangeLists.js');

// Named environment shortcuts for --url flag
const URL_ALIASES = {
  preview: 'https://preview.tealstreet.io',
  v3: 'https://v3.tealstreet.io',
  dev: 'https://dev.tealstreet.io',
};

function resolveUrl(input) {
  if (!input) return '';
  if (URL_ALIASES[input]) return URL_ALIASES[input];
  return input;
}

// Test names (must match test names in spec.cy.ts)
const TEST_NAMES = [
  'Load Balance',
  'Place Market Orders',
  'Place Limit Orders with GTC, Post',
  'Test Position Mode, Margin Type and Leverage',
  'Test reduce orders and opening position',
  'Test Hedge Mode Behavior',
  'Place Bracket Orders',
  'Test Margin Slider',
  'Place stop orders with partial, full close, all trigger types',
];

// Parse command line arguments
const args = process.argv.slice(2);

if (args.includes('--help') || args.includes('-h')) {
  console.log(`
🧪 Cypress E2E Parallel Test Runner

Run E2E tests for multiple exchanges in parallel.

Usage:
  yarn test:e2e:parallel --exchanges <exchange-list>

Options:
  --exchanges <list>    Comma-separated list of exchanges, or "all" (required)
  --tests <list>        Comma-separated list of tests to run (default: all)
  --browser <name>      Browser to use (default: electron)
  --max-parallel <num>  Maximum parallel instances (default: 10)
  --headed              Run with visible browser
  --help, -h            Show this help message

Examples:
  yarn test:e2e:parallel --exchanges okx,woo,blofin
  yarn test:e2e:parallel --exchanges all
  yarn test:e2e:parallel --exchanges okx,woo --tests reduce-orders,place-market-orders
  yarn test:e2e:parallel --exchanges all --max-parallel 8
  yarn test:e2e:parallel --exchanges okx,woo --browser chrome --headed

Available exchanges: ${VALID_EXCHANGES.join(', ')}
All exchanges: ${DEFAULT_EXCHANGES.join(', ')}
`);
  process.exit(0);
}

let exchangeList = [];
let testList = 'all';
let browser = 'electron';
let maxParallel = 4;
let headed = false;
let baseUrl = '';
let record = false;

// Parse flags
for (let i = 0; i < args.length; i++) {
  const arg = args[i];

  if (arg === '--exchanges' && i + 1 < args.length) {
    const exchangesArg = args[i + 1];
    if (exchangesArg === 'all') {
      // "all" = every configured exchange.
      exchangeList = [...DEFAULT_EXCHANGES];
    } else {
      exchangeList = exchangesArg.split(',').map((e) => e.trim());
    }
    i++;
  } else if (arg === '--tests' && i + 1 < args.length) {
    testList = args[i + 1];
    i++;
  } else if (arg === '--browser' && i + 1 < args.length) {
    browser = args[i + 1];
    i++;
  } else if (arg === '--max-parallel' && i + 1 < args.length) {
    maxParallel = parseInt(args[i + 1], 10);
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

// Validate exchanges
if (exchangeList.length === 0) {
  console.error('\n❌ Error: --exchanges flag is required\n');
  console.error('Use --help for usage information\n');
  process.exit(1);
}

const invalidExchanges = exchangeList.filter((ex) => !VALID_EXCHANGES.includes(ex));
if (invalidExchanges.length > 0) {
  console.error(`\n❌ Error: Invalid exchange(s): ${invalidExchanges.join(', ')}\n`);
  console.error('Valid exchanges:', VALID_EXCHANGES.join(', '));
  console.error('');
  process.exit(1);
}

// Results tracking
const results = {
  total: exchangeList.length,
  passed: [],
  failed: [],
  running: 0,
  completed: 0,
  testResults: {}, // Per-exchange, per-test results
};

// Track active child processes and their output buffers for SIGTERM handling
const activeChildren = {}; // { exchange: { process, output } }

const startTime = Date.now();

const coloredExchanges = exchangeList
  .map((ex) => {
    const color = getExchangeColor(ex);
    return `${color}${ex}${colors.reset}`;
  })
  .join(', ');

console.log(`\n${colors.bright}🚀 Starting Parallel E2E Test Runner${colors.reset}`);
console.log(`${colors.dim}${'='.repeat(80)}${colors.reset}`);
console.log(`   ${colors.bright}Exchanges:${colors.reset} ${coloredExchanges}`);
console.log(`   ${colors.bright}Tests:${colors.reset} ${testList}`);
console.log(`   ${colors.bright}Browser:${colors.reset} ${browser}`);
console.log(`   ${colors.bright}Mode:${colors.reset} ${headed ? 'Headed' : 'Headless'}`);
console.log(`   ${colors.bright}Max Parallel:${colors.reset} ${maxParallel}`);
console.log(`   ${colors.bright}Total Exchanges:${colors.reset} ${exchangeList.length}`);
console.log(`${colors.dim}${'='.repeat(80)}${colors.reset}\n`);

/**
 * Strip ANSI escape codes from a string.
 * Cypress embeds color codes (e.g. \x1b[32m✓\x1b[0m) even when piped.
 */
function stripAnsi(str) {
  // eslint-disable-next-line no-control-regex
  return str.replace(/\x1b\[[0-9;]*m/g, '');
}

/**
 * Parse test results from Cypress output
 */
function parseTestResults(output) {
  const cleanOutput = stripAnsi(output);
  const testResults = {};

  TEST_NAMES.forEach((testName) => {
    // Escape special regex characters in test name
    const escapedName = testName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');

    // Check for passing test (✓ followed by test name)
    const passPattern = new RegExp(`✓\\s+${escapedName}`);
    // Check for failing test (number followed by ) and test name)
    const failPattern = new RegExp(`\\d+\\)\\s+${escapedName}`);

    if (passPattern.test(cleanOutput)) {
      testResults[testName] = 'PASS';
    } else if (failPattern.test(cleanOutput)) {
      testResults[testName] = 'FAIL';
    } else {
      testResults[testName] = 'SKIP';
    }
  });

  return testResults;
}

/**
 * Run tests for a single exchange
 */
function runExchangeTests(exchange) {
  return new Promise((resolve) => {
    console.log(`${formatExchangeTag(exchange)} ${colors.dim}Starting tests...${colors.reset}`);
    results.running++;

    const env = {
      ...process.env,
      CYPRESS_EXCHANGE: exchange,
      CYPRESS_ENABLED_TESTS: testList,
    };

    const cypressArgs = [
      'run',
      '--spec',
      'cypress/e2e/spec.cy.ts',
      '--browser',
      browser,
      ...(headed ? ['--headed'] : []),
      ...(record ? ['--record'] : []),
      ...(resolveUrl(baseUrl) ? ['--config', `baseUrl=${resolveUrl(baseUrl)}`] : []),
    ];

    // On Linux, wrap with xvfb-run to give each instance its own X display.
    // --auto-servernum atomically finds a free display, avoiding races between
    // parallel Cypress instances that each try to spawn their own Xvfb.
    const isLinux = process.platform === 'linux' && !headed;
    const command = isLinux ? 'xvfb-run' : 'cypress';
    const spawnArgs = isLinux
      ? ['--auto-servernum', '--server-args=-screen 0 2560x1440x24', 'cypress', ...cypressArgs]
      : cypressArgs;

    const cypress = spawn(command, spawnArgs, {
      env,
      cwd: path.join(__dirname, '../..'),
      stdio: 'pipe', // Capture output instead of inherit to avoid interleaving
    });

    // Track for SIGTERM handling
    const childState = { process: cypress, output: '' };
    activeChildren[exchange] = childState;

    cypress.on('error', (err) => {
      console.error(`${formatExchangeTag(exchange)} ${colors.red}Spawn error:${colors.reset}`, err);
    });

    let output = '';

    // Stream stdout line-by-line with prefix
    const stdoutReader = readline.createInterface({
      input: cypress.stdout,
      terminal: false,
    });

    stdoutReader.on('line', (line) => {
      output += line + '\n';
      childState.output = output;
      // Only print non-empty lines and skip DevTools messages
      if (line.trim() && !line.includes('DevTools listening')) {
        const coloredLine = colorizeOutput(line, exchange);
        console.log(`${formatExchangeTag(exchange)} ${coloredLine}`);
      }
    });

    // Stream stderr line-by-line with prefix
    const stderrReader = readline.createInterface({
      input: cypress.stderr,
      terminal: false,
    });

    stderrReader.on('line', (line) => {
      output += line + '\n';
      childState.output = output;
      // Only print non-empty lines and skip common noise
      if (line.trim() && !line.includes('DevTools listening') && !line.includes('Opening `/dev/tty`')) {
        console.error(`${formatExchangeTag(exchange)} ${colors.dim}${line}${colors.reset}`);
      }
    });

    cypress.on('exit', (code) => {
      delete activeChildren[exchange];
      results.running--;
      results.completed++;

      // Parse individual test results
      const testResults = parseTestResults(output);
      results.testResults[exchange] = testResults;

      if (code === 0) {
        results.passed.push(exchange);
        console.log(
          `${formatExchangeTag(exchange)} ${colors.green}${colors.bright}✅ PASSED${colors.reset} ${colors.dim}(${results.completed}/${results.total})${colors.reset}`,
        );
      } else {
        results.failed.push(exchange);
        console.log(
          `${formatExchangeTag(exchange)} ${colors.red}${colors.bright}❌ FAILED${colors.reset} ${colors.dim}with code ${code} (${results.completed}/${results.total})${colors.reset}`,
        );

        // Save failed output to file for debugging
        const logDir = path.join(__dirname, '../../cypress/logs');
        if (!fs.existsSync(logDir)) {
          fs.mkdirSync(logDir, { recursive: true });
        }
        const logFile = path.join(logDir, `${exchange}-failed-${Date.now()}.log`);
        fs.writeFileSync(logFile, output);
        console.log(`${formatExchangeTag(exchange)} ${colors.yellow}Log saved to: ${logFile}${colors.reset}`);
      }

      resolve(code);
    });
  });
}

/**
 * Run tests in batches with concurrency limit
 */
async function runTestsInBatches() {
  const queue = [...exchangeList];
  const activePromises = [];

  while (queue.length > 0 || activePromises.length > 0) {
    // Start new tests up to max parallel limit
    while (queue.length > 0 && activePromises.length < maxParallel) {
      const exchange = queue.shift();
      const promise = runExchangeTests(exchange);
      activePromises.push(promise);

      // Remove from active when complete
      promise.then(() => {
        const index = activePromises.indexOf(promise);
        if (index > -1) {
          activePromises.splice(index, 1);
        }
      });
    }

    // Wait for at least one to complete before continuing
    if (activePromises.length > 0) {
      await Promise.race(activePromises);
    }
  }

  // Wait for all remaining to complete
  await Promise.all(activePromises);
}

/**
 * Write partial result JSON for an exchange based on buffered stdout.
 * Matches the format written by cypress.config.ts after:run hook.
 */
function writePartialResult(exchange, output) {
  const testResults = parseTestResults(output);
  const tests = TEST_NAMES.map((name) => {
    const state = testResults[name];
    return {
      name,
      state: state === 'PASS' ? 'passed' : state === 'FAIL' ? 'failed' : 'skipped',
      duration: 0,
    };
  }).filter((t) => t.state !== 'skipped');

  const totalPassed = tests.filter((t) => t.state === 'passed').length;
  const totalFailed = tests.filter((t) => t.state === 'failed').length;

  const result = {
    exchange,
    totalPassed,
    totalFailed,
    tests,
    partial: true,
  };

  const resultsDir = path.join(__dirname, '../../cypress/results');
  if (!fs.existsSync(resultsDir)) {
    fs.mkdirSync(resultsDir, { recursive: true });
  }
  const resultFile = path.join(resultsDir, `${exchange}.json`);

  // Only write if no result file exists yet (don't overwrite completed results)
  if (!fs.existsSync(resultFile)) {
    fs.writeFileSync(resultFile, JSON.stringify(result, null, 2));
    console.log(
      `${formatExchangeTag(exchange)} ${colors.yellow}Partial results saved (${totalPassed} passed, ${totalFailed} failed)${colors.reset}`,
    );
  }
}

/**
 * Handle SIGTERM/SIGINT: kill children, write partial results, exit
 */
function handleTermSignal(signal) {
  console.log(`\n${colors.red}${colors.bright}Received ${signal} — writing partial results...${colors.reset}`);

  // Kill all active child processes
  for (const [exchange, child] of Object.entries(activeChildren)) {
    try {
      child.process.kill('SIGKILL');
    } catch (_e) {
      // already dead
    }
    // Write partial results from buffered output
    writePartialResult(exchange, child.output);
  }

  console.log(`${colors.yellow}Partial results written to cypress/results/${colors.reset}`);
  process.exit(1);
}

process.on('SIGTERM', () => handleTermSignal('SIGTERM'));
process.on('SIGINT', () => handleTermSignal('SIGINT'));

// Run all tests
runTestsInBatches().then(() => {
  const duration = ((Date.now() - startTime) / 1000).toFixed(2);

  console.log(`\n${colors.dim}${'='.repeat(80)}${colors.reset}`);
  console.log(`${colors.bright}📊 Test Results Summary${colors.reset}`);
  console.log(`${colors.dim}${'='.repeat(80)}${colors.reset}`);
  console.log(`   ${colors.bright}Total Duration:${colors.reset} ${duration}s`);
  console.log(`   ${colors.bright}Total Exchanges:${colors.reset} ${results.total}`);
  console.log(`   ${colors.bright}Passed:${colors.reset} ${colors.green}${results.passed.length} ✅${colors.reset}`);
  console.log(`   ${colors.bright}Failed:${colors.reset} ${colors.red}${results.failed.length} ❌${colors.reset}`);
  console.log(`${colors.dim}${'='.repeat(80)}${colors.reset}`);

  if (results.passed.length > 0) {
    const passedExchanges = results.passed
      .map((ex) => {
        const color = getExchangeColor(ex);
        return `${color}${ex}${colors.reset}`;
      })
      .join(', ');
    console.log(`\n${colors.green}✅ Passed:${colors.reset} ${passedExchanges}`);
  }

  if (results.failed.length > 0) {
    const failedExchanges = results.failed
      .map((ex) => {
        const color = getExchangeColor(ex);
        return `${color}${ex}${colors.reset}`;
      })
      .join(', ');
    console.log(`\n${colors.red}❌ Failed:${colors.reset} ${failedExchanges}`);
  }

  // Display per-exchange, per-test breakdown
  console.log(`\n${colors.dim}${'='.repeat(80)}${colors.reset}`);
  console.log(`${colors.bright}📋 Detailed Test Results (Per Exchange)${colors.reset}`);
  console.log(`${colors.dim}${'='.repeat(80)}${colors.reset}\n`);

  const allExchanges = [...results.passed, ...results.failed].sort();

  allExchanges.forEach((exchange) => {
    const testResults = results.testResults[exchange];
    if (!testResults) return;

    const passed = Object.values(testResults).filter((r) => r === 'PASS').length;
    const failed = Object.values(testResults).filter((r) => r === 'FAIL').length;
    const skipped = Object.values(testResults).filter((r) => r === 'SKIP').length;

    const exchangeColor = getExchangeColor(exchange);
    console.log(`${exchangeColor}${colors.bright}${exchange.toUpperCase()}${colors.reset}`);
    console.log(`${colors.dim}${'─'.repeat(80)}${colors.reset}`);
    console.log(
      `Summary: ${colors.green}✅ ${passed} passed${colors.reset}, ${colors.red}❌ ${failed} failed${colors.reset}, ${colors.yellow}⊘ ${skipped} skipped${colors.reset}\n`,
    );

    TEST_NAMES.forEach((testName) => {
      const status = testResults[testName];
      let icon, color;

      if (status === 'PASS') {
        icon = '✅';
        color = colors.green;
      } else if (status === 'FAIL') {
        icon = '❌';
        color = colors.red;
      } else {
        icon = '⊘';
        color = colors.yellow;
      }

      console.log(`  ${icon} ${color}${status.padEnd(6)}${colors.reset} ${testName}`);
    });

    console.log('');
  });

  console.log('');

  // Exit with failure code if any tests failed
  process.exit(results.failed.length > 0 ? 1 : 0);
});
