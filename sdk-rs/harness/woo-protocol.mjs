import { createHmac } from 'node:crypto';

const hmac = (text) => createHmac('sha256', 'replay-secret').update(text).digest('hex');
const privateUrl = (url) => new URL(url).pathname.includes('/private/');
export const validateWooRequest = ({ method, url, body, headers = {} }) => {
  const path = new URL(url);
  const fields = Object.fromEntries((Array.isArray(headers) ? headers : Object.entries(headers)).map(([k,v]) => [k.toLowerCase(), String(v)]));
  if (path.pathname.includes('/public/')) {
    if (fields['x-api-signature']) throw new Error('WOO public request was signed');
    return;
  }
  const timestamp = fields['x-api-timestamp'];
  const signature = fields['x-api-signature'];
  // Kline reads are also available without keys.
  if (!signature && /\/(?:hist\/)?kline$/.test(path.pathname)) return;
  if (fields['x-api-key'] !== 'replay-key' || !/^\d+$/.test(timestamp ?? '') || !Number.isSafeInteger(Number(timestamp)) || Math.abs(Number(timestamp) - Date.now()) > 60_000) throw new Error('Invalid WOO API key/timestamp');
  const textBody = body === undefined || body === null ? '' : typeof body === 'string' ? body : JSON.stringify(body);
  const v1 = path.pathname.startsWith('/v1/');
  if (v1) {
    const keys = [...new URLSearchParams(method === 'GET' ? path.search.slice(1) : textBody).keys()];
    if (keys.some((key, i) => i > 0 && keys[i - 1].localeCompare(key) > 0)) throw new Error('WOO V1 parameters are not sorted');
  }
  const payload = v1 ? `${method === 'GET' ? path.search.slice(1) : textBody}|${timestamp}` : `${timestamp}${method}${path.pathname}${path.search}${textBody}`;
  if (signature !== hmac(payload)) throw new Error('Invalid WOO request signature');
  if (v1 && method !== 'GET' && !fields['content-type']?.startsWith('application/x-www-form-urlencoded')) throw new Error('WOO V1 mutation is not form encoded');
};

export const createWooSocketAudit = (_impl, { isPrivate, books = [] }) => {
  const states = new Map();
  const failures = [];
  const state = (url) => {
    if (!states.has(url)) states.set(url, { auth: false, ack: false, topics: new Set(), pings: 0, pongs: 0, pendingPongs: 0 });
    return states.get(url);
  };
  const native = (symbol) => `PERP_${symbol.slice(0, -4)}_${symbol.slice(-4)}`;
  return {
    connected(url) { states.delete(url); state(url); },
    sent(url, text) {
      const message = JSON.parse(text);
      const s = state(url);
      if (message.event === 'auth') {
        const p = message.params;
        s.ack = false;
        s.auth = p?.apikey === 'replay-key' && Number.isSafeInteger(p.timestamp) && Math.abs(p.timestamp - Date.now()) < 60_000 && p.sign === hmac(`|${p.timestamp}`);
        if (!s.auth || !privateUrl(url)) failures.push('invalid WOO private authentication');
      } else if (message.event === 'subscribe') {
        if (privateUrl(url) && !s.ack) failures.push('WOO private subscription preceded authentication');
        if (typeof message.topic !== 'string' || !message.topic) failures.push('invalid WOO subscription topic');
        s.topics.add(message.topic);
      } else if (message.event === 'unsubscribe') s.topics.delete(message.topic);
      else if (message.event === 'ping') s.pings += 1;
      else if (message.event === 'pong') { s.pongs += 1; s.pendingPongs = Math.max(0, s.pendingPongs - 1); }
    },
    received(url, text) {
      const message = JSON.parse(text);
      const s = state(url);
      if (message.event === 'ping') s.pendingPongs += 1;
      if (message.event === 'auth' && message.success === true) {
        if (!s.auth) failures.push('WOO auth acknowledgement preceded a valid signature');
        s.ack = s.auth;
      }
      if (!message.event && message.topic && !s.topics.has(message.topic)) failures.push(`WOO frame arrived without subscription ${message.topic}`);
      if (privateUrl(url) && !message.event && !s.ack) failures.push('WOO private data preceded authentication');
    },
    ready() {
      const entries = [...states.entries()];
      return entries.some(([url,s]) => !privateUrl(url) && ['tickers','bbos','markprices',...books.map((b) => `${native(b)}@orderbookupdate`)].every((topic) => s.topics.has(topic))) &&
        (!isPrivate || entries.some(([url,s]) => privateUrl(url) && s.auth));
    },
    errors() {
      for (const [url,s] of states) {
        if (!s.pings) failures.push(`WOO socket has no heartbeat ${url}`);
        if (s.pendingPongs) failures.push(`WOO server ping was not answered ${url}`);
        if (privateUrl(url) && isPrivate) {
          if (!s.ack) failures.push('WOO private authentication never completed');
          for (const topic of ['executionreport','algoexecutionreportv2']) if (!s.topics.has(topic)) failures.push(`missing WOO subscription ${topic}`);
        }
      }
      return [...new Set(failures)];
    },
  };
};
