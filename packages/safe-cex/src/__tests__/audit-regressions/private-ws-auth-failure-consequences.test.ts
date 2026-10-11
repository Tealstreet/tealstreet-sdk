import { ExchangeHealthMonitor } from '../../health/ExchangeHealthMonitor';
import { WsDirectFallback } from '../../utils/wsDirectFallback';

/**
 * Two consequences of a failed private-WS auth that are invisible from the socket
 * file, and that each cost a review round to find.
 *
 * 1. The socket deliberately sends NOTHING when the remote signer is gone, so the
 *    connection exchanges no frames. `WsDirectFallback` reads a zero-message
 *    connect as "this egress path is broken" and, after two of them, advances onto
 *    the relay and then the athena passthrough FOR THE SOCKET'S LIFETIME — putting
 *    every signer-less user on one shared IP against the venue's per-IP connection
 *    cap, which is precisely what that class's own comment says must never happen.
 *    The silence is evidence about our signer, not about the route.
 *
 * 2. The reported message must not read as a rejected API key. The literal
 *    'auth failed' is an `auth_invalid` pattern, severity critical, and the web
 *    controller turns that into a red "key rejected" toast AND suppresses
 *    automatic account reloads for five minutes — the very reload that re-pairs
 *    the browser signer. A backgrounded tab or one 15s sign timeout must not look
 *    like a bad credential. The classifier had no test for this at all, so the
 *    regression was one word away.
 */

describe('a failed private WS auth does not escalate the egress chain', () => {
  const PRIMARY = 'wss://stream.bybit.com/v5/private';
  const RELAY = 'wss://relay.example/ts-relay';
  const ATHENA = 'wss://bybit-proxy-ja.tealstreet.io/v5/private';

  it('advances after two genuinely silent connects', () => {
    const chain = new WsDirectFallback();

    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
    // Third attempt: two consecutive silent connects are now behind us.
    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(RELAY);
  });

  it('stays on the primary when the silence was ours', () => {
    const chain = new WsDirectFallback();

    for (let attempt = 0; attempt < 6; attempt++) {
      expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
      // What the socket does when it declines to send anything on a dead signer.
      chain.disregardSilence();
    }
  });

  it('still advances when only some attempts were ours', () => {
    const chain = new WsDirectFallback();

    chain.resolveUrl(PRIMARY, RELAY, ATHENA);
    chain.disregardSilence();
    // Two real silent connects after the disregarded one.
    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
    expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(RELAY);
  });

  // A real inbound frame must keep working as the reset, or a wrong API key would
  // start tripping the chain.
  it('treats a real message as proof the path works', () => {
    const chain = new WsDirectFallback();

    for (let attempt = 0; attempt < 6; attempt++) {
      expect(chain.resolveUrl(PRIMARY, RELAY, ATHENA)).toBe(PRIMARY);
      chain.onMessage();
    }
  });
});

describe('a signer failure is not reported as a rejected API key', () => {
  // `processMessage` returns the ErrorCategory directly (or null), and the error
  // path is what the socket takes — `emitAndLogError`, not a plain log.
  const classify = (exchangeName: string, message: string) => {
    const monitor = new ExchangeHealthMonitor(exchangeName);
    try {
      return monitor.processMessage(message, true);
    } finally {
      monitor.dispose();
    }
  };

  const TERMINAL = ['auth_invalid', 'auth_expired', 'auth_permissions'];

  // Every venue whose private WS asks a remote signer. bybit is the template;
  // bitget/blofin/okx/phemex/woo/kraken/mexc/bitunix were swept to
  // match, and `errorPatterns.ts`'s `EXCHANGE_SPECIFIC_PATTERNS` cannot shadow
  // this default without being caught here.
  const VENUES = ['bybit', 'bitget', 'blofin', 'okx', 'phemex', 'woo', 'kraken', 'mexc', 'bitunix'];

  // The control FIRST: this is the wording the fix removed, and it must still
  // classify under every venue — otherwise the assertions below pass for the
  // wrong reason.
  it.each(VENUES)('classifies the old wording as a terminal auth failure under %s', (exchange) => {
    expect(TERMINAL).toContain(classify(exchange, 'WS auth failed'));
  });

  // The strings the sockets actually emit on the two signer failure modes.
  const EMITTED = [
    'Remote WS signing unavailable: browser closed mid-signature',
    'Remote signer unavailable — WebSocket disconnected (gone <5s)',
    'Signing request V1StGXR8Z5jdHi6BmyT timed out after 15000ms',
    'WebSocket disconnected — signing aborted',
    'Failed to send signing request',
  ];

  const CASES = VENUES.flatMap((exchange) => EMITTED.map((message) => ({ exchange, message })));

  it.each(CASES)('does not classify "$message" as a terminal auth failure under $exchange', ({ exchange, message }) => {
    expect(TERMINAL).not.toContain(classify(exchange, message));
  });
});
