export const OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS = new Set(['Http', 'NodeFetch', 'Fetch']);

export const SERVICE_DROPPED_INTEGRATIONS = new Set([
  'Console',
  ...OUTGOING_REQUEST_BREADCRUMB_INTEGRATIONS,
  'RequestData',
  'OnUncaughtException',
  'OnUnhandledRejection',
  'BunServer',
  'BunHttpServer',
]);
