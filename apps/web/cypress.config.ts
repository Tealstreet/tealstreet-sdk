import fs from 'fs';
import path from 'path';

import { defineConfig } from 'cypress';

// eslint-disable-next-line @typescript-eslint/no-require-imports
const exchangeConfigs = require('./cypress/config/exchanges.js');

// Store load failure diagnostics for inclusion in results JSON. Shape:
//   { exchange, phase: 'env' | 'auth' | 'visit' | 'load', error?, summary?,
//     loadingFlags, orderbookSpinning, consoleErrors, failedRequests,
//     transportFailureReason?, transportFailureUrl? }
let loadFailureDiagnostics: any = null;

// Store per-test failure diagnostics (request log entries captured on test failure)
const testFailureDiagnostics: Record<string, any> = {};

// Top-level transport-failure flag for the result JSON. Populated whenever
// either logLoadFailure or storeTestFailureDiagnostics carries a
// `transportFailureReason`. First-match-wins so the bucketing logic in
// slack-e2e-report.js sees the earliest detected reason.
let transportFailureReason: string | null = null;
let transportFailureUrl: string | null = null;

// Store transport config (proxy/tunnel settings) for diagnostics
let transportConfig: Record<string, any> | null = null;

type E2EAccountConfig = Record<string, any>;

export const isHostedE2EBaseUrl = (baseUrl?: string | null): boolean => {
  if (!baseUrl) return false;
  try {
    const host = new URL(baseUrl).hostname;
    return host !== 'localhost' && host !== '127.0.0.1' && host !== '0.0.0.0';
  } catch {
    return false;
  }
};

export const normalizeTargetAccountForE2ERuntime = (
  targetExchange: string,
  account: E2EAccountConfig,
  options: { baseUrl?: string | null; useExchangeServer?: unknown },
): E2EAccountConfig => {
  if (
    targetExchange === 'woo' &&
    String(options.useExchangeServer) === 'true' &&
    isHostedE2EBaseUrl(options.baseUrl) &&
    !account.tealstreetTunnel
  ) {
    return { ...account, tealstreetProxy: true };
  }
  return account;
};

export default defineConfig({
  // Cypress Cloud project ID - get from https://cloud.cypress.io after creating a project
  // Leave undefined to run without Cypress Cloud recording
  projectId: process.env.CYPRESS_PROJECT_ID || undefined,

  e2e: {
    baseUrl: 'http://localhost:3000',
    setupNodeEvents(on, config) {
      // Comprehensive logging tasks for debugging
      on('task', {
        log(message) {
          console.log(message);
          return null;
        },
        logError(message) {
          console.error('❌', message);
          return null;
        },
        logWarning(message) {
          console.warn('⚠️ ', message);
          return null;
        },
        logNetwork(data) {
          const { method, url, status, request, response } = data;
          console.log(`\n${'='.repeat(80)}`);
          console.log(`🌐 ${method} ${url}`);
          console.log(`📊 Status: ${status}`);
          if (request) {
            console.log(`📤 Request:`, typeof request === 'object' ? JSON.stringify(request, null, 2) : request);
          }
          if (response) {
            console.log(`📥 Response:`, typeof response === 'object' ? JSON.stringify(response, null, 2) : response);
          }
          console.log(`${'='.repeat(80)}\n`);
          return null;
        },
        logWebSocket(data) {
          const { event, url, message } = data;
          console.log(`\n${'~'.repeat(80)}`);
          console.log(`🔌 WebSocket ${event}: ${url}`);
          if (message) {
            console.log(`💬 Message:`, typeof message === 'object' ? JSON.stringify(message, null, 2) : message);
          }
          console.log(`${'~'.repeat(80)}\n`);
          return null;
        },
        logLoadFailure(diagnostics) {
          loadFailureDiagnostics = diagnostics;
          // Hoist transport-failure tags to the top-level result JSON so
          // bucketExchange() in slack-e2e-report.js can read ex.transportFailureReason
          // without nested access. First-match-wins.
          if (diagnostics?.transportFailureReason && !transportFailureReason) {
            transportFailureReason = diagnostics.transportFailureReason;
            transportFailureUrl = diagnostics.transportFailureUrl || null;
          }
          const phaseTag = diagnostics?.phase ? `[phase=${diagnostics.phase}] ` : '';
          console.error(`\n${'!'.repeat(80)}`);
          console.error(phaseTag + (diagnostics.summary || JSON.stringify(diagnostics, null, 2)));
          console.error(`${'!'.repeat(80)}\n`);
          return null;
        },
        storeTestFailureDiagnostics({
          testName,
          entries,
          failedRequests,
          storeSnapshot,
          transportFailureReason: testTransportReason,
          transportFailureUrl: testTransportUrl,
        }: {
          testName: string;
          entries: any[];
          failedRequests: any[];
          storeSnapshot?: Record<string, any> | null;
          transportFailureReason?: string;
          transportFailureUrl?: string;
        }) {
          testFailureDiagnostics[testName] = entries;
          if (failedRequests && failedRequests.length > 0) {
            testFailureDiagnostics[testName + '__failedHttp'] = failedRequests;
          }
          if (storeSnapshot && Object.keys(storeSnapshot).length > 0) {
            testFailureDiagnostics[testName + '__storeSnapshot'] = storeSnapshot;
          }
          if (testTransportReason && !transportFailureReason) {
            transportFailureReason = testTransportReason;
            transportFailureUrl = testTransportUrl || null;
          }
          return null;
        },
        storeTransportConfig(config: Record<string, any>) {
          transportConfig = config;
          return null;
        },
        /**
         * Append one machine-readable ground-truth event.
         *
         * Distinct from `writeApiLog`, which is prose for a human debugging a failure.
         * This is the record of what the suite DELIBERATELY did — action, size, side,
         * and the position observed afterwards — so an accounting verifier can later
         * pull the venue's own history for the same window and assert it reconstructs
         * exactly these events.
         *
         * That independence is the point: every history check in this repo so far
         * compares exchange data against exchange data, which agrees by construction.
         * This is the only oracle that knows what actually happened.
         */
        recordGroundTruth({ exchange, event }: { exchange: string; event: Record<string, any> }) {
          const logDir = path.join(process.cwd(), 'cypress', 'logs');
          fs.mkdirSync(logDir, { recursive: true });
          const logFile = path.join(logDir, `${exchange}-ground-truth.jsonl`);
          fs.appendFileSync(logFile, `${JSON.stringify(event)}\n`);
          return null;
        },
        writeApiLog({ exchange, testName, entries }: { exchange: string; testName: string; entries: any[] }) {
          if (!entries || entries.length === 0) return null;
          const logDir = path.join(process.cwd(), 'cypress', 'logs');
          fs.mkdirSync(logDir, { recursive: true });
          const logFile = path.join(logDir, `${exchange}-api.log`);
          const header = `\n${'='.repeat(80)}\n[${new Date().toISOString()}] ${testName}\n${'='.repeat(80)}\n`;
          // Entries come from the app's grafana event system:
          // { timestamp: number, type: 'httpRequest'|'httpResponse'|'error'|'rest', severity: string, message: string }
          const lines = entries
            .map((e: any) => {
              const ts = typeof e.timestamp === 'number' ? new Date(e.timestamp).toISOString() : e.timestamp;
              const tag = e.type === 'error' ? 'ERROR' : (e.type || 'unknown').toUpperCase();
              // Truncate very long messages (full JSON responses) to keep log readable
              const msg =
                typeof e.message === 'string'
                  ? e.message.substring(0, 4000)
                  : JSON.stringify(e.message).substring(0, 4000);
              return `[${ts}] [${tag}] ${msg}`;
            })
            .join('\n');
          fs.appendFileSync(logFile, header + lines + '\n');
          return null;
        },
      });

      // Chrome flags for headless CI environments (small VMs, no GPU)
      on('before:browser:launch', (browser, launchOptions) => {
        if (browser.name === 'chrome') {
          launchOptions.args.push('--no-sandbox');
          launchOptions.args.push('--disable-dev-shm-usage');
          // Force viewport size in headless mode (headless Chrome ignores Cypress viewport config)
          launchOptions.args.push('--window-size=2560,1440');
          // Use normal user agent to avoid exchange API blocks on HeadlessChrome UA
          if (browser.isHeadless) {
            launchOptions.args.push(
              '--user-agent=Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/144.0.0.0 Safari/537.36',
            );
          }
        }
        return launchOptions;
      });

      // Increase Node memory limit to prevent buffer overflow
      on('before:run', () => {
        // This runs in Node, not browser
        if (process.env.NODE_OPTIONS) {
          process.env.NODE_OPTIONS += ' --max-old-space-size=4096';
        } else {
          process.env.NODE_OPTIONS = '--max-old-space-size=4096';
        }
      });

      // Write per-test results JSON after run completes (for CI Slack reporting)
      on('after:run', (results) => {
        if (!process.env.CI) return;
        const exchange = process.env.CYPRESS_EXCHANGE || 'unknown';
        // Cypress reports every test in a spec as state==='failed', duration===0 once the
        // spec aborts (e.g. beforeEach throws or a real failure halts the run). We relabel
        // those zero-duration phantom failures as state==='blocked' so reports can tell
        // them apart from real regressions. `priorTestExecuted` flips on the first test
        // that actually ran (pass or real-duration fail) — anything blocked before that
        // means the spec never got past beforeEach/before.
        const tests: {
          name: string;
          state: string;
          duration: number;
          blockedReason?: string;
          error?: string;
        }[] = [];
        if (results && 'runs' in results) {
          for (const run of (results as CypressCommandLine.CypressRunResult).runs) {
            let priorTestExecuted = false;
            for (const test of run.tests) {
              const name = test.title.filter(Boolean).pop() || 'unknown';
              const duration = test.duration;
              let state: string = test.state;
              let blockedReason: string | undefined;
              if ((state === 'failed' || state === 'skipped') && duration === 0) {
                state = 'blocked';
                blockedReason = priorTestExecuted ? 'spec aborted after prior failure' : 'beforeEach/before failed';
              } else if ((state === 'passed' || state === 'failed') && duration > 0) {
                priorTestExecuted = true;
              }
              const displayError = (test as { displayError?: string }).displayError;
              const error = displayError ? displayError.split('\n')[0].slice(0, 500) : undefined;
              tests.push({
                name,
                state,
                duration,
                ...(blockedReason ? { blockedReason } : {}),
                ...(error ? { error } : {}),
              });
            }
          }
        }
        const totalPassed = tests.filter((t) => t.state === 'passed').length;
        const totalBlocked = tests.filter((t) => t.state === 'blocked').length;
        const totalFailed = tests.filter((t) => t.state === 'failed' || t.state === 'blocked').length;
        const runUrl =
          results && 'runUrl' in results ? (results as CypressCommandLine.CypressRunResult).runUrl : undefined;
        const output = {
          exchange,
          totalPassed,
          totalFailed,
          totalBlocked,
          tests,
          ...(runUrl ? { runUrl } : {}),
          ...(transportConfig ? { transportConfig } : {}),
          ...(transportFailureReason ? { transportFailureReason } : {}),
          ...(transportFailureUrl ? { transportFailureUrl } : {}),
          ...(loadFailureDiagnostics ? { loadFailureDiagnostics } : {}),
          ...(Object.keys(testFailureDiagnostics).length > 0 ? { testFailureDiagnostics } : {}),
        };
        const resultsDir = path.join(process.cwd(), 'cypress', 'results');
        fs.mkdirSync(resultsDir, { recursive: true });
        fs.writeFileSync(path.join(resultsDir, `${exchange}.json`), JSON.stringify(output, null, 2));
      });

      // Inject exchange configuration (normal E2E test mode)
      const targetExchange = process.env.CYPRESS_EXCHANGE || config.env.account?.exchange || 'okx';
      console.log(`[Cypress] Using exchange: ${targetExchange}`);

      try {
        const exchangeConfig = exchangeConfigs[targetExchange];
        if (!exchangeConfig) {
          throw new Error(`No configuration found for exchange: ${targetExchange}`);
        }
        config.env.exchangeConfig = exchangeConfig;

        // Find matching account from cypress.env.json
        const allAccounts = config.env.allAccounts || [];
        const targetAccount = allAccounts.find((a: any) => a.exchange === targetExchange);

        if (targetAccount) {
          const normalizedAccount = normalizeTargetAccountForE2ERuntime(targetExchange, targetAccount, {
            baseUrl: config.baseUrl,
            useExchangeServer: config.env.useExchangeServer,
          });
          config.env.account = normalizedAccount;
          console.log(`[Cypress] Using account: ${targetAccount.name}`);
        } else {
          throw new Error(
            `No account found for exchange: ${targetExchange}. ` +
              'Ensure the CYPRESS_ENV_JSON secret (or local cypress.env.json) contains ' +
              `an account with exchange: "${targetExchange}" and active: true.`,
          );
        }
      } catch (error: any) {
        console.error(`[Cypress] Error loading exchange config: ${error.message}`);
        throw error;
      }

      return config;
    },
    // Suppress network request logs
    // @ts-ignore
    experimentalNetworkStubbing: true,
  },
  watchForFileChanges: false,
  viewportWidth: 2560,
  viewportHeight: 1440,
  chromeWebSecurity: false,
  // Prevent 100MB buffer overflow from WebSocket data
  numTestsKeptInMemory: 0, // Don't keep any tests in memory
  video: false,
  experimentalMemoryManagement: true,
  // Reduce timeouts to run tests faster
  defaultCommandTimeout: 10000,
  pageLoadTimeout: 60000,
});
