#!/usr/bin/env node

/**
 * Aggregates E2E test result JSONs and sends a summary to Slack.
 *
 * Sends a clean pass/fail summary, then uploads a detailed diagnostics
 * file (request logs, store snapshots, exchange errors) for easy download.
 *
 * Usage: node slack-e2e-report.js <results-directory>
 *
 * Each file in the results directory is a JSON with per-test results:
 *   { exchange, totalPassed, totalFailed, tests: [{ name, state, duration }] }
 *
 * Environment variables:
 *   SLACK_E2E_BOT_TOKEN    - Slack bot token (xoxb-...)
 *   SLACK_E2E_CHANNEL_ID   - Slack channel ID
 *   GITHUB_RUN_ID          - GitHub Actions run ID
 *   GITHUB_REPOSITORY      - owner/repo
 *   GITHUB_SERVER_URL      - e.g. https://github.com
 *   GITHUB_ACTOR           - user who triggered the run
 *   GITHUB_EVENT_NAME      - "schedule" or "workflow_dispatch"
 *   E2E_TARGET_URL         - target URL tests ran against
 *   CYPRESS_PROJECT_ID     - Cypress Cloud project ID (for linking to results)
 *   SLACK_E2E_DM_USER_ID   - Optional: send report as DM to this Slack user ID instead of channel
 */

const fs = require('fs');
const path = require('path');

function formatDuration(ms) {
  if (ms < 1000) return `${ms}ms`;
  const seconds = Math.round(ms / 1000);
  if (seconds < 60) return `${seconds}s`;
  const minutes = Math.floor(seconds / 60);
  const remainingSeconds = seconds % 60;
  return `${minutes}m ${remainingSeconds}s`;
}

/**
 * Post a message to Slack using the Web API (bot token).
 */
async function postSlackMessage(token, channelId, blocks, text, threadTs) {
  const body = { channel: channelId, text: text || '' };
  if (blocks) body.blocks = blocks;
  if (threadTs) body.thread_ts = threadTs;

  const res = await fetch('https://slack.com/api/chat.postMessage', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(body),
  });

  const data = await res.json();
  if (!data.ok) {
    throw new Error(`chat.postMessage failed: ${data.error}`);
  }
  return data;
}

/**
 * Upload a file to Slack using files.uploadV2 flow.
 */
async function uploadSlackFile(token, channelId, content, filename, title, threadTs) {
  // Step 1: Get upload URL
  const getUrlRes = await fetch('https://slack.com/api/files.getUploadURLExternal', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/x-www-form-urlencoded',
    },
    body: new URLSearchParams({
      filename,
      length: Buffer.byteLength(content, 'utf8').toString(),
    }),
  });

  const getUrlData = await getUrlRes.json();
  if (!getUrlData.ok) {
    throw new Error(`files.getUploadURLExternal failed: ${getUrlData.error}`);
  }

  const fileId = getUrlData.file_id;

  // Step 2: Upload content
  const uploadRes = await fetch(getUrlData.upload_url, {
    method: 'POST',
    headers: { 'Content-Type': 'text/plain' },
    body: content,
  });

  if (!uploadRes.ok) {
    throw new Error(`File upload failed: ${uploadRes.status}`);
  }

  // Step 3: Complete upload — share to channel thread
  const completeBody = {
    files: [{ id: fileId, title }],
    channel_id: channelId,
  };
  if (threadTs) {
    completeBody.thread_ts = threadTs;
  }

  const completeRes = await fetch('https://slack.com/api/files.completeUploadExternal', {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify(completeBody),
  });

  const completeData = await completeRes.json();
  if (!completeData.ok) {
    throw new Error(`files.completeUploadExternal failed: ${completeData.error}`);
  }

  // completeUploadExternal processes sharing asynchronously —
  // wait before exiting so Slack has time to finish sharing the file.
  console.log('Waiting 15s for Slack to process file sharing...');
  await new Promise((r) => setTimeout(r, 15000));

  return completeData;
}

/**
 * Classify an exchange result into one of five buckets so the report can
 * separate real regressions from infra outages.
 *
 * Priority: clean > transport-flap > blocked-at-load > partial > clean-failure.
 * `transportFailureReason` is populated by Wave B2; until then this branch
 * stays cold and exchanges with TradeTunnel/proxy issues fall into
 * `blocked-at-load` via the loadFailureDiagnostics path.
 */
function bucketExchange(ex) {
  const failed = ex.totalFailed || 0;
  const passed = ex.totalPassed || 0;
  const blocked = ex.totalBlocked || 0;
  const totalRan = passed + failed;
  // A spec that crashed before any test ran has failed === 0 too; it is not clean.
  if (totalRan === 0) return ex.transportFailureReason ? 'transport-flap' : 'blocked-at-load';
  if (failed === 0) return 'clean';
  if (ex.transportFailureReason) return 'transport-flap';
  if (ex.loadFailureDiagnostics && blocked >= failed - 1) return 'blocked-at-load';
  if (passed > 0 && failed > 0) return 'partial';
  return 'clean-failure';
}

const BUCKET_ORDER = ['clean', 'blocked-at-load', 'transport-flap', 'partial', 'clean-failure'];
const BUCKET_EMOJI = {
  clean: ':white_check_mark:',
  'blocked-at-load': ':construction:',
  'transport-flap': ':satellite_antenna:',
  partial: ':large_yellow_circle:',
  'clean-failure': ':x:',
};

function computeBucketCounts(exchanges) {
  const counts = { clean: 0, partial: 0, 'blocked-at-load': 0, 'transport-flap': 0, 'clean-failure': 0 };
  for (const ex of exchanges) {
    counts[bucketExchange(ex)]++;
  }
  return counts;
}

/**
 * Build Slack summary with per-test detail per exchange (green/red circles + duration).
 */
function buildSlackBlocks(exchanges, totalPassed, totalTests, allPassed, runUrl, cypressCloudUrl, triggerText) {
  const targetUrl = process.env.E2E_TARGET_URL || 'unknown';

  const blocks = [
    {
      type: 'header',
      text: {
        type: 'plain_text',
        text: allPassed
          ? `[v3][e2e] E2E Tests Passed (${totalPassed}/${totalTests} tests)`
          : `[v3][e2e] E2E Tests Failed (${totalPassed}/${totalTests} tests passed)`,
        emoji: true,
      },
    },
  ];

  // Rollup buckets — separate real regressions from infra outages at a glance.
  const buckets = computeBucketCounts(exchanges);
  const rollupParts = BUCKET_ORDER.map((key) => `${buckets[key]} ${key} ${BUCKET_EMOJI[key]}`);
  blocks.push({
    type: 'section',
    text: { type: 'mrkdwn', text: `*${rollupParts.join('  •  ')}*` },
  });

  // Per-exchange sections — one section block per exchange to stay under
  // Slack's 3000 character limit per section text field.
  for (const ex of exchanges) {
    const totalRan = (ex.totalPassed || 0) + (ex.totalFailed || 0);
    const specCrashed = totalRan === 0;
    // A spec that crashed before any test ran has totalRan === 0; we don't want to render
    // it as a green-ish "(0/0 passed)" so we mark it with :x: and a denominator only.
    const exchangeIcon = specCrashed ? ':x:' : ex.totalFailed === 0 ? ':white_check_mark:' : ':x:';
    const name = ex.runUrl ? `<${ex.runUrl}|${ex.exchange.toUpperCase()}>` : ex.exchange.toUpperCase();
    let transportSuffix = '';
    const tc = ex.transportConfig;
    if (tc) {
      const parts = [];
      if (tc.proxy) parts.push(`Proxy: ${tc.proxyRegion || 'on'}`);
      if (tc.tunnel) parts.push('Tunnel: on');
      if (parts.length > 0) transportSuffix = `  — _${parts.join(', ')}_`;
    }

    const countSuffix = specCrashed ? '(0/0)' : `(${ex.totalPassed}/${ex.totalPassed + ex.totalFailed} passed)`;
    // Prepend a satellite emoji when the run hit a transport-side failure so
    // operators can spot infra outages at a glance vs scrolling per-test.
    const transportPrefix = ex.transportFailureReason ? ':satellite_antenna: ' : '';
    let section = `${exchangeIcon}  ${transportPrefix}*${name}*  ${countSuffix}${transportSuffix}`;

    if (specCrashed) {
      section += `\n      :warning: spec crashed before any test ran`;
    }

    if (ex.transportFailureReason) {
      // Collapse per-test exchange-error spam down to a single transport line.
      // The per-test green/red icons still render below — those are useful —
      // but a transport-side outage will be 1 line, not N copies of the same
      // "TradeTunnel connection lost" body.
      const tfUrl = ex.transportFailureUrl ? ` (${ex.transportFailureUrl})` : '';
      section += `\n      :satellite_antenna: Transport: ${ex.transportFailureReason}${tfUrl}`;
    }

    if (ex.totalFailed > 0) {
      const diag = ex.loadFailureDiagnostics;
      if (diag) {
        const reasonText = diag.failureReason || diag.error;
        if (reasonText) {
          const phaseSuffix = diag.phase ? ` (phase: ${diag.phase})` : '';
          section += `\n      :warning: ${reasonText}${phaseSuffix}`;
        }
      }
    }

    if (ex.tests && ex.tests.length > 0) {
      const testLines = ex.tests
        .map((t) => {
          const icon =
            t.state === 'passed'
              ? ':large_green_circle:'
              : t.state === 'blocked'
                ? ':black_circle:'
                : t.state === 'skipped'
                  ? ':white_circle:'
                  : ':red_circle:';
          const duration = formatDuration(t.duration);
          const blockedSuffix = t.state === 'blocked' ? `  [BLOCKED — ${t.blockedReason || 'unknown'}]` : '';
          const errorSuffix = t.state === 'failed' && t.error ? `\n         _${t.error.slice(0, 200)}_` : '';
          return `      ${icon} ${t.name}  _(${duration})_${blockedSuffix}${errorSuffix}`;
        })
        .join('\n');
      section += '\n' + testLines;
    }

    blocks.push({
      type: 'section',
      text: { type: 'mrkdwn', text: section },
    });
  }

  // Context line
  blocks.push({
    type: 'context',
    elements: [
      {
        type: 'mrkdwn',
        text: `Target: ${targetUrl}  |  Trigger: ${triggerText}  |  <!date^${Math.floor(Date.now() / 1000)}^{date_short_pretty} at {time}|${new Date().toISOString()}>`,
      },
    ],
  });

  // Action buttons
  const buttons = [];
  if (cypressCloudUrl) {
    buttons.push({
      type: 'button',
      text: { type: 'plain_text', text: 'Cypress Cloud', emoji: true },
      url: cypressCloudUrl,
      style: 'primary',
    });
  }
  buttons.push({
    type: 'button',
    text: { type: 'plain_text', text: 'GitHub Run', emoji: true },
    url: runUrl,
  });
  blocks.push({ type: 'actions', elements: buttons });

  return blocks;
}

/**
 * Build a detailed plain-text diagnostics report for download.
 */
function buildDetailedReport(exchanges, totalPassed, totalTests, runUrl, triggerText) {
  const targetUrl = process.env.E2E_TARGET_URL || 'unknown';
  const lines = [];

  lines.push('='.repeat(80));
  lines.push(`E2E Test Diagnostics Report`);
  lines.push(`Generated: ${new Date().toISOString()}`);
  lines.push(`Target: ${targetUrl}`);
  lines.push(`Trigger: ${triggerText}`);
  lines.push(`Results: ${totalPassed}/${totalTests} tests passed`);
  const buckets = computeBucketCounts(exchanges);
  lines.push(`Buckets: ${BUCKET_ORDER.map((k) => `${buckets[k]} ${k}`).join(' / ')}`);
  lines.push(`GitHub Run: ${runUrl}`);
  lines.push('='.repeat(80));
  lines.push('');

  for (const ex of exchanges) {
    const status = ex.totalFailed === 0 && ex.totalPassed > 0 ? 'PASSED' : 'FAILED';
    lines.push('-'.repeat(80));
    lines.push(`${ex.exchange.toUpperCase()} — ${status} (${ex.totalPassed}/${ex.totalPassed + ex.totalFailed})`);
    if (ex.runUrl) lines.push(`Cypress Cloud: ${ex.runUrl}`);
    const tc = ex.transportConfig;
    if (tc) {
      lines.push(
        `Transport: proxy=${tc.proxy} tunnel=${tc.tunnel}${tc.proxyRegion ? ' region=' + tc.proxyRegion : ''}`,
      );
    }
    if (ex.transportFailureReason) {
      lines.push(
        `Transport Failure: ${ex.transportFailureReason}${ex.transportFailureUrl ? ' at ' + ex.transportFailureUrl : ''}`,
      );
    }
    lines.push('-'.repeat(80));

    // Test results
    if (ex.tests && ex.tests.length > 0) {
      lines.push('');
      lines.push('Tests:');
      for (const t of ex.tests) {
        const icon =
          t.state === 'passed' ? 'PASS' : t.state === 'blocked' ? 'BLOCKED' : t.state === 'skipped' ? 'SKIP' : 'FAIL';
        const dur = formatDuration(t.duration);
        const blockedSuffix = t.state === 'blocked' ? ` — ${t.blockedReason || 'unknown'}` : '';
        lines.push(`  [${icon}] ${t.name} (${dur})${blockedSuffix}`);
        if (t.state === 'failed' && t.error) {
          lines.push(`      ${t.error}`);
        }
      }
    }

    // Load failure diagnostics
    const diag = ex.loadFailureDiagnostics;
    if (diag) {
      lines.push('');
      lines.push('Load Failure Diagnostics:');
      if (diag.phase) {
        lines.push(`  Phase: ${diag.phase}`);
      }
      if (diag.failureReason) {
        lines.push(`  Reason: ${diag.failureReason}`);
      }
      if (diag.error) {
        lines.push(`  Error: ${diag.error}`);
      }
      if (diag.loadingFlags) {
        const flags = Object.entries(diag.loadingFlags)
          .map(([key, loaded]) => `${loaded ? 'OK' : 'MISSING'} ${key}`)
          .join(', ');
        lines.push(`  Loading flags: ${flags}`);
      }
      if (diag.orderbookSpinning) {
        lines.push('  Orderbook: spinning (not loaded)');
      }
      if (diag.consoleErrors && diag.consoleErrors.length > 0) {
        lines.push(`  Console errors (${diag.consoleErrors.length}):`);
        diag.consoleErrors.forEach((e) => lines.push(`    ${e}`));
      }
      if (diag.failedRequests && diag.failedRequests.length > 0) {
        lines.push(`  Failed requests (${diag.failedRequests.length}):`);
        diag.failedRequests.forEach((req) => {
          lines.push(`    ${req.method} ${req.url} -> ${req.status}`);
          if (req.body) lines.push(`      ${req.body.slice(0, 500)}`);
        });
      }
    }

    // Per-test failure diagnostics
    const testDiag = ex.testFailureDiagnostics;
    if (testDiag && Object.keys(testDiag).length > 0) {
      lines.push('');
      lines.push('Test Failure Diagnostics:');

      for (const [key, entries] of Object.entries(testDiag)) {
        if (key.endsWith('__failedHttp')) {
          const testLabel = key.replace('__failedHttp', '');
          lines.push(`  [${testLabel}] Failed HTTP requests:`);
          (entries || []).forEach((req) => {
            lines.push(`    ${req.method} ${req.url} -> ${req.status}`);
            if (req.body) lines.push(`      ${req.body.slice(0, 1000)}`);
          });
        } else if (key.endsWith('__storeSnapshot')) {
          const testLabel = key.replace('__storeSnapshot', '');
          const snapshot = entries;
          if (snapshot && Object.keys(snapshot).length > 0) {
            lines.push(`  [${testLabel}] Store snapshot:`);
            for (const [acct, data] of Object.entries(snapshot)) {
              const d = data || {};
              const orders = d.orders || [];
              const positions = d.positions || [];
              lines.push(`    ${acct}: ${orders.length} order(s), ${positions.length} position(s)`);
              orders.forEach((o) => {
                lines.push(`      ${o.side} ${o.type} ${o.symbol} ${o.amount}@${o.price} [${o.status}]`);
              });
              positions.forEach((p) => {
                lines.push(`      POS ${p.side} ${p.symbol} ${p.contracts} contracts @${p.entryPrice}`);
              });
            }
          }
        } else {
          // Request log entries
          const allEntries = entries || [];
          if (allEntries.length > 0) {
            lines.push(`  [${key}] Relevant requests (${allEntries.length}):`);
            allEntries.forEach((e) => {
              if (e.type === 'exchangeError' || e.type === 'exchangeWarning') {
                lines.push(`    >> [${e.type}] ${(e.message || '').slice(0, 1000)}`);
              } else {
                lines.push(`    [${e.severity || e.type}] ${(e.message || '').slice(0, 1000)}`);
              }
            });
          }
        }
      }
    }

    lines.push('');
  }

  return lines.join('\n');
}

async function main() {
  const resultsDir = process.argv[2];
  if (!resultsDir) {
    console.error('Usage: node slack-e2e-report.js <results-directory>');
    process.exit(1);
  }

  // Read result JSON files
  const exchanges = [];
  if (fs.existsSync(resultsDir)) {
    for (const file of fs.readdirSync(resultsDir)) {
      const filePath = path.join(resultsDir, file);
      if (fs.statSync(filePath).isFile()) {
        try {
          const data = JSON.parse(fs.readFileSync(filePath, 'utf8'));
          exchanges.push(data);
        } catch {
          const outcome = fs.readFileSync(filePath, 'utf8').trim();
          exchanges.push({
            exchange: path.basename(file, path.extname(file)),
            totalPassed: outcome === 'success' ? 1 : 0,
            totalFailed: outcome === 'success' ? 0 : 1,
            tests: [],
          });
        }
      }
    }
  }

  if (exchanges.length === 0) {
    console.log('No result files found. Skipping Slack report.');
    return;
  }

  // Sort alphabetically
  exchanges.sort((a, b) => a.exchange.localeCompare(b.exchange));

  // Calculate totals
  let totalTests = 0;
  let totalPassed = 0;
  let totalFailed = 0;
  for (const ex of exchanges) {
    totalTests += ex.totalPassed + ex.totalFailed;
    totalPassed += ex.totalPassed;
    totalFailed += ex.totalFailed;
  }

  const crashedSpecs = exchanges.filter((ex) => ex.totalPassed + ex.totalFailed === 0).length;
  const allPassed = totalFailed === 0 && crashedSpecs === 0;

  // Build URLs
  const runUrl = `${process.env.GITHUB_SERVER_URL || 'https://github.com'}/${process.env.GITHUB_REPOSITORY}/actions/runs/${process.env.GITHUB_RUN_ID}`;
  const cypressProjectId = process.env.CYPRESS_PROJECT_ID;
  const cypressCloudUrl = cypressProjectId ? `https://cloud.cypress.io/projects/${cypressProjectId}/runs` : null;

  // Determine trigger info
  const eventName = process.env.GITHUB_EVENT_NAME || 'unknown';
  const actor = process.env.GITHUB_ACTOR || 'unknown';
  const triggerText = eventName === 'schedule' ? 'Scheduled (daily)' : `Manual by ${actor}`;

  // Build messages
  const blocks = buildSlackBlocks(exchanges, totalPassed, totalTests, allPassed, runUrl, cypressCloudUrl, triggerText);
  const detailedReport = buildDetailedReport(exchanges, totalPassed, totalTests, runUrl, triggerText);
  const summaryText = `[v3][e2e] E2E: ${totalPassed}/${totalTests} passed across ${exchanges.length} exchanges`;

  // Send to Slack — DM mode sends to a user instead of channel
  const token = process.env.SLACK_E2E_BOT_TOKEN;
  const channelId = process.env.SLACK_E2E_CHANNEL_ID;
  const target = process.env.SLACK_E2E_DM_USER_ID || channelId;

  if (!token || !target) {
    console.log('SLACK_E2E_BOT_TOKEN or SLACK_E2E_CHANNEL_ID not set. Printing payload instead:');
    console.log(JSON.stringify({ blocks }, null, 2));
    console.log('\n--- Detailed Report ---\n');
    console.log(detailedReport);
    return;
  }

  if (process.env.SLACK_E2E_DM_USER_ID) {
    console.log(`DM mode: sending to user ${process.env.SLACK_E2E_DM_USER_ID}`);
  }

  // Post summary message
  const msgResult = await postSlackMessage(token, target, blocks, summaryText);
  console.log(`Slack summary sent to ${target} (channel=${msgResult.channel}, ts=${msgResult.ts}): ${summaryText}`);

  // Always upload diagnostics file as a reply in the same thread
  const timestamp = new Date().toISOString().replace(/[:.]/g, '-').slice(0, 19);
  const filename = `e2e-diagnostics-${timestamp}.txt`;
  const title = `E2E Diagnostics — ${totalPassed}/${totalTests} passed`;

  // files.completeUploadExternal requires a real channel ID, not a user ID.
  // chat.postMessage resolves user IDs to DM channel IDs in the response.
  const fileChannel = msgResult.channel || target;
  try {
    await uploadSlackFile(token, fileChannel, detailedReport, filename, title, msgResult.ts);
    console.log(`Diagnostics file uploaded to ${fileChannel} (thread ${msgResult.ts}): ${filename}`);
  } catch (err) {
    console.error('Failed to upload diagnostics file:', err.message);
  }

  // Always write to disk for GitHub artifact
  const reportPath = path.join(resultsDir, 'detailed-diagnostics.txt');
  fs.writeFileSync(reportPath, detailedReport);
  console.log(`Detailed report written to: ${reportPath}`);
}

main().catch((err) => {
  console.error('Error sending Slack report:', err);
  process.exit(1);
});
