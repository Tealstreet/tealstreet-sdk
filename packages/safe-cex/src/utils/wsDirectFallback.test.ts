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
