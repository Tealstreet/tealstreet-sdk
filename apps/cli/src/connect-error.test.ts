import { describe, expect, it } from 'vitest';

import { classifyConnectError } from './connect-error';

/**
 * Regression coverage for the shared connect-failure classifier used by both
 * the REPL background-connect handler and `runSingleCommand`. Covers a
 * user-correctable auth failure and an unexpected/non-auth failure (the repo
 * guideline on error-behavior changes), plus the case-insensitivity that was
 * the original bug — `Authentication failed` must not fall through to generic.
 */
describe('classifyConnectError', () => {
  it('classifies user-correctable auth failures (case-insensitive)', () => {
    expect(classifyConnectError('authentication failed')).toBe('auth');
    expect(classifyConnectError('Authentication failed. Token expired.')).toBe('auth');
    expect(classifyConnectError('Invalid TOKEN')).toBe('auth');
    expect(classifyConnectError('HTTP 401 Unauthorized')).toBe('auth');
  });

  it('classifies timeout failures (both "timed out" and "timeout", any case)', () => {
    expect(classifyConnectError('Request timed out')).toBe('timeout');
    expect(classifyConnectError('Connection TIMED OUT')).toBe('timeout');
    expect(classifyConnectError('socket timeout')).toBe('timeout');
  });

  it('classifies unexpected/non-auth failures as generic', () => {
    expect(classifyConnectError('ECONNREFUSED 127.0.0.1:443')).toBe('generic');
    expect(classifyConnectError('Unexpected server response: 503')).toBe('generic');
    expect(classifyConnectError('')).toBe('generic');
  });

  it('prefers auth over timeout when a message mentions both', () => {
    // Auth is the more actionable signal (prompts "Run login"), so it wins.
    expect(classifyConnectError('authentication timed out')).toBe('auth');
  });
});
