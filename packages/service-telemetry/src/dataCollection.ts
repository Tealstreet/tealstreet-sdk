/**
 * Restrictive Sentry v11 data collection baseline for code that handles user
 * exchange accounts or credentials.
 *
 * Intentionally left unannotated: current v10 typings lack some v11 fields
 * (`queues`, `stackFrameVariables`), and assigning a named value avoids the
 * v10 excess-property check while still sending the v11-supported switches.
 */
export const RESTRICTIVE_SENTRY_DATA_COLLECTION = {
  userInfo: false,
  cookies: false,
  httpHeaders: {
    request: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
    response: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
  },
  httpBodies: [],
  urlQueryParams: { deny: ['forwarded', '-ip', 'remote-', 'via', '-user'] },
  genAI: { inputs: false, outputs: false },
  databaseQueryData: false,
  graphQL: { document: false, variables: false },
  queues: false,
  stackFrameVariables: false,
};
