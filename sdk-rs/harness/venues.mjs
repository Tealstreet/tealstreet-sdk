// How the harness drives each venue: the TS exchange class it replays
// against, and the name the Rust engine is built with.
import { createHmac } from 'node:crypto';

const root = '../../packages/safe-cex/src/exchanges';

export const VENUES = {
  cryptocom: {
    rust: 'cryptocom',
    ts: { module: root + '/cryptocom/cryptocom.exchange.ts', className: 'CryptocomExchange', name: 'cryptocom' },
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('book.')).length >= count,
    privateSocket: '/exchange/v1/user',
    // KNOWN_TS_FIXES: the global ticker/book throttles drop updates across
    // symbols. Flush each pending trailing call so the reference sees all data.
    afterTsFrame: (exchange) => {
      exchange.publicWebsocket.handleTickerUpdate.flush();
      exchange.publicWebsocket.handleBookUpdate.flush();
    },
    isWrite: (method, url) => method === 'POST' && /private\/(advanced\/)?(create|amend|cancel|change|close)-/.test(new URL(url).pathname),
    createSocketAudit: (impl, { isPrivate, books }) => {
      const connections = new Map();
      const failures = [];
      const socket = (url) => {
        if (!connections.has(url)) connections.set(url, { auth: false, authAck: false, channels: new Set(), heartbeats: new Set() });
        return connections.get(url);
      };
      return {
        received(url, text) {
          const frame = JSON.parse(text);
          if (frame.method === 'public/heartbeat') socket(url).heartbeats.add(frame.id);
          if (frame.method === 'public/auth' && frame.code === 0) {
            if (!socket(url).auth) failures.push('auth acknowledgement preceded valid auth request');
            socket(url).authAck = socket(url).auth;
          }
        },
        sent(url, text) {
          const frame = JSON.parse(text);
          const state = socket(url);
          if (frame.method === 'public/auth') {
            const payload = frame.method + frame.id + frame.api_key + frame.nonce;
            state.auth = Number.isSafeInteger(frame.id) && Number.isSafeInteger(frame.nonce) && Math.abs(Date.now() - frame.nonce) < 60_000 && frame.api_key === 'replay-key' && frame.sig === createHmac('sha256', 'replay-secret').update(payload).digest('hex');
            if (!state.auth) failures.push('invalid private WS auth signature');
          } else if (frame.method === 'subscribe') {
            if (url.endsWith('/user') && !state.authAck) failures.push('private subscriptions sent before authentication');
            for (const channel of frame.params.channels) state.channels.add(channel);
          } else if (frame.method === 'unsubscribe') {
            for (const channel of frame.params.channels) state.channels.delete(channel);
          } else if (frame.method === 'public/respond-heartbeat') {
            if (!state.heartbeats.delete(frame.id)) failures.push('unexpected heartbeat response id');
          }
        },
        ready() {
          const entries = [...connections.entries()];
          if (isPrivate && !entries.some(([url, state]) => url.endsWith('/user') && state.auth)) return false;
          if (books.length && !entries.some(([url, state]) => url.endsWith('/market') && books.every((symbol) => state.channels.has('book.' + symbol + '.50')))) return false;
          return true;
        },
        errors() {
          for (const [url, state] of connections) {
            if (state.heartbeats.size) failures.push('heartbeat response missing');
            if (url.endsWith('/user') && isPrivate) {
              if (!state.auth) failures.push('private WS never authenticated');
              const required = ['user.order', 'user.advance.order', 'user.positions', 'user.balance', ...(impl === 'rust' ? ['user.trade'] : [])];
              for (const channel of required) if (!state.channels.has(channel)) failures.push('missing private subscription ' + channel);
            }
            if (url.endsWith('/market')) for (const symbol of books) {
              if (!state.channels.has('book.' + symbol + '.50')) failures.push('missing book subscription ' + symbol);
            }
          }
          return failures;
        },
      };
    },
    checkRequest: (method, url, body) => {
      const path = new URL(url).pathname.replace('/exchange/v1/', '');
      if (!path.startsWith('private/')) return true;
      try {
        const envelope = JSON.parse(body);
        const params = Object.keys(envelope.params).sort().map((key) => {
          const value = envelope.params[key];
          return key + (Array.isArray(value) ? value.join('') : String(value));
        }).join('');
        const payload = envelope.method + envelope.id + envelope.api_key + params + envelope.nonce;
        return method === 'POST' && envelope.method === path && Number.isSafeInteger(envelope.id) && Number.isSafeInteger(envelope.nonce) && Math.abs(Date.now() - envelope.nonce) < 60_000 && envelope.api_key === 'replay-key' &&
          createHmac('sha256', 'replay-secret').update(payload).digest('hex') === envelope.sig;
      } catch { return false; }
    },
  },
  binance: {
    rust: 'binance',
    ts: { module: `${root}/binance/binance.exchange.ts`, className: 'BinanceExchange', name: 'binance' },
    /** Depth topics are registered once each book subscription reaches the socket. */
    tsBooksSubscribed: (exchange, count) =>
      Object.keys(exchange.publicWebsocket.messageHandlers).filter((key) => key.startsWith('depthUpdate.')).length >= count,
    privateSocket: '/private/ws',
    isWrite: (method, url) => method !== 'GET' && !new URL(url).pathname.endsWith('/listenKey'),
  },
};

export const venueOf = (name) => {
  const venue = VENUES[name];
  if (!venue) throw new Error(`unknown venue ${name}; known: ${Object.keys(VENUES).join(', ')}`);
  return venue;
};

/** Every axios instance the exchange owns, so all of its REST goes to the tape. */
export const axiosClients = (exchange) =>
  [...new Set(Object.values(exchange))].filter((value) => value?.defaults && value?.interceptors);
