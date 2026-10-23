import { describe, expect, it } from 'vitest';

import { WsDirectFallback } from './wsDirectFallback';

const DIRECT = 'wss://direct.example';
const RELAY = 'wss://relay.example/tunnel?ts-relay=x';
const ATHENA = 'wss://athena.example';

describe('WsDirectFallback', () => {
  it('dials direct until two consecutive connects were silent, then sticks to the fallback', () => {
    const f = new WsDirectFallback();
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(DIRECT); // connect 1: silent
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(DIRECT); // connect 2: silent
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(ATHENA); // two silent connects -> fallback
    f.onMessage();
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(ATHENA); // sticky
  });

  it('any message on a connection resets the silent streak', () => {
    const f = new WsDirectFallback();
    f.resolveUrl(DIRECT, ATHENA); // connect 1: silent
    f.resolveUrl(DIRECT, ATHENA); // connect 2 ...
    f.onMessage(); // ... answered (even a pong or an auth error counts)
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(DIRECT); // streak reset -> still direct
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(DIRECT); // one silent connect is not enough
    expect(f.resolveUrl(DIRECT, ATHENA)).toBe(ATHENA);
  });

  it('walks a chain: relay after the direct streak, athena after a silent relay streak, then sticks', () => {
    const f = new WsDirectFallback();
    f.resolveUrl(DIRECT, RELAY, ATHENA);
    f.resolveUrl(DIRECT, RELAY, ATHENA);
    expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(RELAY); // direct silent x2
    expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(RELAY); // relay silent x1: not yet
    expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(ATHENA); // relay silent x2 -> athena
    for (let i = 0; i < 4; i++) expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(ATHENA); // last: sticky
  });

  it('a relay that answers keeps the socket on the relay', () => {
    const f = new WsDirectFallback();
    f.resolveUrl(DIRECT, RELAY, ATHENA);
    f.resolveUrl(DIRECT, RELAY, ATHENA);
    expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(RELAY);
    for (let i = 0; i < 4; i++) {
      f.onMessage(); // each relay connection answers
      expect(f.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(RELAY);
    }
  });

  it('skips undefined candidates so gated entries need no branching at the call site', () => {
    const f = new WsDirectFallback();
    f.resolveUrl(DIRECT, undefined, ATHENA);
    f.resolveUrl(DIRECT, undefined, ATHENA);
    expect(f.resolveUrl(DIRECT, undefined, ATHENA)).toBe(ATHENA);
  });

  it('never leaves the primary when the route declares no fallback', () => {
    const f = new WsDirectFallback();
    for (let i = 0; i < 5; i++) expect(f.resolveUrl(DIRECT, undefined)).toBe(DIRECT);
  });
});

describe('host fallback recovery policy', () => {
  it('matches the frozen Bitget inline counters for all 512 nine-message patterns over ten connects', () => {
    for (let pattern = 0; pattern < 512; pattern++) {
      let lastMessageCount = 0;
      let messageCount = 0;
      let connectionAttempts = 0;
      let useFallbackUrl = false;
      const fallback = new WsDirectFallback(2, 1);
      for (let attempt = 0; attempt < 10; attempt++) {
        // Frozen oracle from the TS Bitget socket classes, independently of the
        // generic helper: the first connect itself counts as a silent attempt.
        if (lastMessageCount === 0 && messageCount === 0) {
          connectionAttempts++;
          if (connectionAttempts >= 2) useFallbackUrl = true;
        }
        lastMessageCount = messageCount;
        messageCount = 0;
        expect(fallback.resolveUrl(DIRECT, ATHENA), `pattern=${pattern} attempt=${attempt}`).toBe(
          useFallbackUrl ? ATHENA : DIRECT,
        );
        if (pattern & (1 << attempt)) {
          messageCount++;
          fallback.onMessage();
        }
      }
    }
  });

  it('an explicit recovery signal sticks to the final declared candidate', () => {
    const fallback = new WsDirectFallback();
    expect(fallback.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(DIRECT);
    fallback.onMessage();
    fallback.preferFallback();
    expect(fallback.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(ATHENA);
    fallback.onMessage();
    expect(fallback.resolveUrl(DIRECT, RELAY, ATHENA)).toBe(ATHENA);
    const direct = new WsDirectFallback();
    direct.preferFallback();
    expect(direct.resolveUrl(DIRECT)).toBe(DIRECT);
  });

  describe('tunnel thresholds', () => {
    it('ignores events from a retired attempt', () => {
      const fallback = new WsDirectFallback(2, 0, { silentOpens: 2, failedConnects: 6 });
      fallback.primaryTunnel = true;
      fallback.resolveUrl('proxy', 'relay');
      const retired = fallback.attempt;
      for (let attempt = 0; attempt < 5; attempt++) expect(fallback.resolveUrl('proxy', 'relay')).toBe('proxy');
      fallback.onOpen(retired);
      fallback.onMessage(retired);
      expect(fallback.resolveUrl('proxy', 'relay')).toBe('relay');
    });

    const PROXY = 'wss://proxy.example';
    const tunnel = () => {
      const f = new WsDirectFallback(2, 0, { silentOpens: 2, failedConnects: 6 });
      f.primaryTunnel = true;
      return f;
    };

    it('one relay strike does not leave the relay; a second silent open does', () => {
      const f = tunnel();
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(PROXY);
      expect(f.rejectCurrent()).toBe(true);
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(RELAY);
      f.onOpen(); // strike 1: opened, closed before any frame
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(RELAY);
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(RELAY); // a failed connect is a separate count
      f.onOpen();
      f.onMessage(); // a login answer resets silent opens
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(RELAY);
      f.onOpen();
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(RELAY);
      f.onOpen();
      expect(f.resolveUrl(PROXY, RELAY, ATHENA)).toBe(ATHENA);
    });

    it('switching candidates starts both counts over', () => {
      const f = tunnel();
      for (let attempt = 0; attempt < 6; attempt++) expect(f.resolveUrl(PROXY, RELAY)).toBe(PROXY);
      expect(f.resolveUrl(PROXY, RELAY)).toBe(RELAY);
      f.onOpen();
      expect(f.resolveUrl(PROXY, RELAY)).toBe(RELAY);
    });

    it('a direct primary keeps the silent-connect rule', () => {
      const f = new WsDirectFallback(2, 0, { silentOpens: 2, failedConnects: 6 });
      expect(f.resolveUrl(DIRECT, RELAY)).toBe(DIRECT);
      expect(f.resolveUrl(DIRECT, RELAY)).toBe(DIRECT);
      expect(f.resolveUrl(DIRECT, RELAY)).toBe(RELAY);
    });
  });
});
