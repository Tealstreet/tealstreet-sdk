#!/usr/bin/env node

/**
 * Generate test summary from Cypress logs
 *
 * Usage:
 *   node generate-test-summary.js <log-file-or-directory>
 */

const fs = require('fs');
const path = require('path');

const testNames = [
  'Load Balance',
  'Place Market Orders',
  'Place Limit Orders with GTC, Post',
  'Test Position Mode, Margin Type and Leverage',
  'Test reduce orders and opening position',
  'Test Hedge Mode Behavior',
  'Place Bracket Orders',
  'Test Margin Slider',
  'Place stop orders with partial, full close, all trigger types'
];

function parseLogFile(filePath, exchange) {
  const content = fs.readFileSync(filePath, 'utf8');
  const results = {};

  testNames.forEach(testName => {
    // Check for passing test
    const passPattern = new RegExp(`✓\\s+${testName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`);
    const failPattern = new RegExp(`\\d+\\)\\s+${testName.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`);

    if (passPattern.test(content)) {
      results[testName] = 'PASS';
    } else if (failPattern.test(content)) {
      results[testName] = 'FAIL';
    } else {
      results[testName] = 'SKIP';
    }
  });

  return results;
}

function printSummary(exchangeResults) {
  console.log('\n' + '='.repeat(120));
  console.log('📊 E2E Test Summary - Per Exchange, Per Test');
  console.log('='.repeat(120) + '\n');

  const exchanges = Object.keys(exchangeResults).sort();

  exchanges.forEach(exchange => {
    const results = exchangeResults[exchange];
    const passed = Object.values(results).filter(r => r === 'PASS').length;
    const failed = Object.values(results).filter(r => r === 'FAIL').length;
    const skipped = Object.values(results).filter(r => r === 'SKIP').length;

    console.log(`\n${exchange.toUpperCase()}`);
    console.log('-'.repeat(120));
    console.log(`Summary: ✅ ${passed} passed, ❌ ${failed} failed, ⊘ ${skipped} skipped\n`);

    testNames.forEach(testName => {
      const status = results[testName];
      let icon, color;

      if (status === 'PASS') {
        icon = '✅';
        color = '\x1b[32m'; // green
      } else if (status === 'FAIL') {
        icon = '❌';
        color = '\x1b[31m'; // red
      } else {
        icon = '⊘';
        color = '\x1b[33m'; // yellow
      }

      console.log(`  ${icon} ${color}${status.padEnd(6)}\x1b[0m ${testName}`);
    });

    console.log('');
  });

  // Overall summary
  console.log('='.repeat(120));
  console.log('Overall Summary');
  console.log('='.repeat(120));

  const allTests = [];
  exchanges.forEach(exchange => {
    Object.entries(exchangeResults[exchange]).forEach(([test, result]) => {
      allTests.push({ exchange, test, result });
    });
  });

  const totalPassed = allTests.filter(t => t.result === 'PASS').length;
  const totalFailed = allTests.filter(t => t.result === 'FAIL').length;
  const totalSkipped = allTests.filter(t => t.result === 'SKIP').length;
  const totalTests = allTests.length;

  console.log(`Total Tests: ${totalTests}`);
  console.log(`✅ Passed: ${totalPassed}`);
  console.log(`❌ Failed: ${totalFailed}`);
  console.log(`⊘ Skipped: ${totalSkipped}`);
  console.log('='.repeat(120) + '\n');
}

// Main
const args = process.argv.slice(2);
if (args.length === 0) {
  console.error('Usage: node generate-test-summary.js <log-file-or-directory>');
  process.exit(1);
}

const input = args[0];
const exchangeResults = {};

if (fs.statSync(input).isDirectory()) {
  // Process all log files in directory
  const files = fs.readdirSync(input).filter(f => f.endsWith('.log'));

  files.forEach(file => {
    const match = file.match(/^([a-z]+)-/);
    if (match) {
      const exchange = match[1];
      const filePath = path.join(input, file);
      exchangeResults[exchange] = parseLogFile(filePath, exchange);
    }
  });
} else {
  // Process single file
  const match = path.basename(input).match(/^([a-z]+)-/);
  if (match) {
    const exchange = match[1];
    exchangeResults[exchange] = parseLogFile(input, exchange);
  }
}

printSummary(exchangeResults);
