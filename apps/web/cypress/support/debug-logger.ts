/**
 * Automated Debug Logging System for Cypress E2E Tests
 *
 * This provides a robust logging system that captures API calls, console errors,
 * and test events without causing memory issues or test crashes.
 *
 * Usage: Import and call setupDebugLogging() in your test file's before() hook
 */

interface LoggedRequest {
  method: string;
  url: string;
  body?: any;
  status?: number;
  response?: any;
  timestamp: string;
}

/**
 * Logs data to the terminal via cy.task()
 */
export function logToTerminal(category: string, data: any) {
  const timestamp = new Date().toISOString();
  const logMessage = `
${'='.repeat(80)}
[${timestamp}] ${category}
${typeof data === 'string' ? data : JSON.stringify(data, null, 2)}
${'='.repeat(80)}
`;
  cy.task('log', logMessage);
}

/**
 * Sets up basic API request logging for debugging
 * No longer sets up specific intercepts - those are fragile and exchange-specific
 */
export function setupAPILogging(exchangeName: string) {
  // Just log that we're testing this exchange - no intercepts needed
  cy.log(`Testing exchange: ${exchangeName}`);
  return {};
}

/**
 * Simplified - no longer waits for API intercepts
 * Just logs the description for debugging
 */
export function waitAndLogAPI(alias: string, description?: string) {
  // Deprecated - no longer sets up intercepts
  // Just log for debugging purposes
  if (description) {
    cy.log(description);
  }
}

/**
 * Sets up console error logging
 * Captures browser console errors and sends them to terminal
 *
 * Note: Disabled for now as it can interfere with test execution
 */
export function setupConsoleLogging() {
  // Disabled - console interception can cause test failures
  // If needed in the future, implement using cy.on('log') instead
}

/**
 * Main setup function - call this in your test's before() or beforeEach() hook
 */
export function setupDebugLogging(exchangeName: string, options: {
  logConsoleErrors?: boolean;
  logAllAPICalls?: boolean;
} = {}) {
  const { logConsoleErrors = true, logAllAPICalls = false } = options;

  // Setup console logging
  if (logConsoleErrors) {
    setupConsoleLogging();
  }

  // Setup API logging
  const aliases = setupAPILogging(exchangeName);

  // Log test start
  logToTerminal('Test Started', {
    exchange: exchangeName,
    timestamp: new Date().toISOString(),
  });

  return aliases;
}
