import { describe, expect, it } from 'vitest';

import { WsDirectFallback } from './wsDirectFallback';

const DIRECT = 'wss://direct.example';
const FALLBACK = 'wss://fallback.example';

describe('WsDirectFallback', () => {
  it('dials direct until two consecutive connects were silent, then sticks to the fallback', () => {
    const f = new WsDirectFallback();
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(DIRECT); // connect 1: silent
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(DIRECT); // connect 2: silent
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(FALLBACK); // two silent connects -> fallback
    f.onMessage();
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(FALLBACK); // sticky
  });

  it('any message on a connection resets the silent streak', () => {
    const f = new WsDirectFallback();
    f.resolveUrl(DIRECT, FALLBACK); // connect 1: silent
    f.resolveUrl(DIRECT, FALLBACK); // connect 2 ...
    f.onMessage(); // ... answered (even a pong or an auth error counts)
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(DIRECT); // streak reset -> still direct
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(DIRECT); // one silent connect is not enough
    expect(f.resolveUrl(DIRECT, FALLBACK)).toBe(FALLBACK);
  });

  it('never leaves the primary when the route declares no fallback', () => {
    const f = new WsDirectFallback();
    for (let i = 0; i < 5; i++) expect(f.resolveUrl(DIRECT, undefined)).toBe(DIRECT);
  });
});
