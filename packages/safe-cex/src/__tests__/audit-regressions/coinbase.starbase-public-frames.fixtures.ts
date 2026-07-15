import { STARBASE_PUBLIC_TICKER_FIXTURE } from './coinbase.starbase-public.fixtures';

/**
 * Normalized deterministic fixtures derived from independently captured live
 * public envelopes. They are not byte-for-byte raw captures; the source raw
 * frame hashes are pinned in coinbase.starbase-public-frames.ts and docs.
 */
export const STARBASE_CAPTURE_DERIVED_PUBLIC_NOTIFICATION_FRAME = JSON.stringify({
  jsonrpc: '2.0',
  method: 'subscription',
  params: {
    channel: 'ticker.BTC_USDC-PERPETUAL.100ms',
    data: STARBASE_PUBLIC_TICKER_FIXTURE,
  },
});

export const STARBASE_CAPTURE_DERIVED_HEARTBEAT_FRAME = JSON.stringify({
  jsonrpc: '2.0',
  method: 'heartbeat',
  params: { type: 'heartbeat' },
});

export const STARBASE_CAPTURE_DERIVED_TEST_REQUEST_FRAME = JSON.stringify({
  jsonrpc: '2.0',
  method: 'heartbeat',
  params: { type: 'test_request' },
});

export const STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_RESULT_FRAME = JSON.stringify({
  jsonrpc: '2.0',
  id: 41,
  result: { version: '1.2.26' },
  testnet: false,
  usDiff: 100,
  usIn: 1_784_113_251_374_106,
  usOut: 1_784_113_251_374_206,
});

export const STARBASE_CAPTURE_DERIVED_PUBLIC_TEST_ERROR_FRAME = JSON.stringify({
  jsonrpc: '2.0',
  id: 42,
  error: { code: -32601, data: 'Method not found', message: 'Method not found' },
});
