// A private recorder for EngineHost's transport interface. Only sanitized copies
// enter the tape; the engine and the real transport keep the original bytes.
import { gzipSync, gunzipSync, deflateSync, inflateSync, deflateRawSync, inflateRawSync } from 'node:zlib';
import { canonicalUrl } from './requests.mjs';
import { POLYMARKET_REPLAY } from './polymarket.mjs';
import { ASTER_PRIVATE_KEY, ASTER_USER, ASTER_SIGNER } from './asterdex-requests.mjs';

const normalized = (name) => String(name).split('.').at(-1).toLowerCase().replace(/[-_]/g, '');
const credentialNames = new Set(['key', 'apikey', 'secret', 'apisecret', 'privatekey', 'passphrase', 'password', 'authorization', 'cookie', 'setcookie']);
const sessionNames = new Set(['token', 'accesstoken', 'refreshtoken', 'authtoken', 'sessiontoken', 'sessionkey', 'listenkey', 'jwt', 'jwttoken', 'auth', 'bearer']);
const signatureNames = new Set(['signature', 'sig', 'sign', 'signedchallenge', 'authent']);
const sensitive = (name) => credentialNames.has(normalized(name)) || sessionNames.has(normalized(name)) || signatureNames.has(normalized(name)) || /(?:apikey|apisecret|passphrase|accesstoken|authtoken|refreshtoken|signature|apisign)$/.test(normalized(name));
const parseJson = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? JSON.rawJSON(context.source) : value);
const textOf = (value) => {
  if (value instanceof URLSearchParams) return value.toString();
  if (value instanceof ArrayBuffer) return new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(value));
  if (ArrayBuffer.isView(value)) return new TextDecoder('utf-8', { fatal: true }).decode(value);
  return typeof value === 'string' ? value : JSON.stringify(value);
};

/** Known secrets plus discovered auth/session fields are scrubbed before write. */
export function createSmokeRedactor(secrets = {}, replayCredentials = {}, { venue } = {}) {
  const aliases = new Map();
  const configuredNames = new Set(Object.keys(secrets).map(normalized));
  let next = 0;
  const remember = (name, value) => {
    if (typeof value !== 'string' || !value || aliases.has(value) || [...aliases.values()].includes(value)) return;
    const key = normalized(name);
    if (key.endsWith('authorization')) {
      const bearer = /^Bearer\s+(.+)$/i.exec(value);
      if (bearer) remember('token', bearer[1]);
    }
    if (key === 'cookie' || key === 'setcookie') {
      for (const part of value.split(';')) {
        const at = part.indexOf('=');
        if (at > 0 && !/^(?:path|domain|expires|max-age|samesite)$/i.test(part.slice(0, at).trim())) remember('sessiontoken', part.slice(at + 1).trim());
      }
    }
    const override = String(name).split('.').reduce((value, part) => value?.[part], replayCredentials);
    const alias = typeof override === 'string' ? override
      : /^0x[\da-f]{40}$/i.test(value) ? `0x${'11'.repeat(20)}`
        : /^0x[\da-f]{64}$/i.test(value) ? `0x${'01'.repeat(32)}`
          : ['key', 'apikey'].includes(key) ? 'replay-key'
      : ['secret', 'apisecret', 'privatekey'].includes(key) ? 'replay-secret'
        : ['passphrase', 'password'].includes(key) ? 'replay-passphrase'
          : signatureNames.has(key) ? '<redacted-signature>'
            : `live-smoke-${sessionNames.has(key) ? 'session' : 'secret'}-${++next}`;
    aliases.set(value, alias);
  };
  const discover = (value, name = '') => {
    if (typeof value === 'string') {
      if (sensitive(name)) remember(name, value);
      try {
        const url = new URL(value);
        if (url.username) remember('key', decodeURIComponent(url.username));
        if (url.password) remember('password', decodeURIComponent(url.password));
        for (const [key, item] of url.searchParams) if (sensitive(key)) remember(key, item);
      } catch { /* Plain strings and non-URL payloads. */ }
      if (/^[\s]*[\[{\"]/.test(value)) {
        try { discover(parseJson(value)); } catch { /* Plain text. */ }
      } else if (/(?:^|&)[A-Za-z_%-][^=&]*=/.test(value)) {
        for (const [key, item] of new URLSearchParams(value)) discover(item, key);
      }
    } else if (value && typeof value === 'object' && !JSON.isRawJSON(value)) {
      const method = String(value.method ?? value.op ?? '').toLowerCase();
      if (method === 'auth' && Array.isArray(value.args) && value.args.length === 3) {
        remember('key', value.args[0]); remember('signature', value.args[2]);
      }
      if (method === 'user.auth' && Array.isArray(value.params)) {
        remember('key', value.params[1]); remember('signature', value.params[2]);
      }
      if (Array.isArray(value.signature)) {
        remember('key', value.signature[0]); remember('signature', value.signature[1]);
      } else if (value.signature && typeof value.signature === 'object') {
        for (const item of Object.values(value.signature)) remember('signature', item);
      }
      for (const [key, item] of Object.entries(value)) discover(item, key);
    }
  };
  // Extra secret labels need not themselves be standard option names.
  for (const [key, value] of Object.entries(secrets)) {
    if (typeof value === 'string') remember(key, value);
    else discover(value);
  }
  const replace = (text) => {
    const variants = [];
    for (const [secret, alias] of aliases) {
      const encoded = encodeURIComponent(secret);
      const forms = new Set([secret, encoded, encoded.replace(/%[\dA-F]{2}/g, (part) => part.toLowerCase()),
        JSON.stringify(secret).slice(1, -1), Buffer.from(secret).toString('base64'), Buffer.from(secret).toString('base64url')]);
      for (const form of forms) if (form) variants.push([form, alias]);
    }
    // Longer encodings first: replacing a substring must not leave a secret suffix.
    variants.sort(([a], [b]) => b.length - a.length);
    for (const [secret, alias] of variants) {
      if (secret.length < 4) { /* Short account indices collide with valid prices/order IDs; redact only their named credential fields. */ }
      else text = text.split(secret).join(alias);
    }
    return text;
  };
  const identity = (value, name, path = []) => {
    if (venue !== 'lighter') return value;
    const field = normalized(name);
    const credential = ['accountindex', 'applicationid'].includes(field) || (field === 'index' && path.some((part) => ['accounts', 'account'].includes(part)))
      ? 'applicationId' : field === 'apikeyindex' ? 'apiKeyIndex' : undefined;
    if (credential && String(value) === secrets[credential]) {
      const alias = replayCredentials[credential];
      return typeof value === 'number' ? Number(alias) : alias;
    }
    if (typeof value === 'string' && ['channel', 'subscription'].includes(field)) {
      return value.replace(/^(account_all(?:_[a-z_]+)?[/:])(\d+)(?=$|\/)/, (whole, prefix, index) =>
        index === secrets.applicationId ? prefix + replayCredentials.applicationId : whole);
    }
    return value;
  };
  const visit = (value, name = '', path = []) => {
    value = identity(value, name, path);
    if (typeof value === 'string') {
      if (sensitive(name) || configuredNames.has(normalized(name))) return aliases.get(value) ?? replace(value);
      if (/^[\s]*[\[{\"]/.test(value)) {
        try { return JSON.stringify(visit(parseJson(value))); } catch { /* Plain text. */ }
      }
      if (/^[A-Za-z_%][\w%.-]*=/.test(value)) {
        return new URLSearchParams([...new URLSearchParams(value)].map(([key, item]) => [key, visit(item, key, [...path, key])])).toString();
      }
      return replace(value);
    }
    if (JSON.isRawJSON(value)) return value;
    if (Array.isArray(value)) return value.map((item) => visit(item, '', path));
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [replace(key), visit(item, key, [...path, key])]));
    return value;
  };
  const redact = (value) => { discover(value); return visit(value); };
  const headers = (value = '') => {
    const entries = typeof value === 'string'
      ? value.split(/\r?\n/).filter(Boolean).map((line) => { const at = line.indexOf(':'); return [line.slice(0, at), line.slice(at + 1).trim()]; })
      : value instanceof Headers ? [...value] : Array.isArray(value) ? value : Object.entries(value);
    for (const [name, item] of entries) if (sensitive(name)) remember(name, String(item));
    return entries.map(([name, item]) => `${name}: ${sensitive(name) ? aliases.get(String(item)) : replace(String(item))}`).join('\n');
  };
  const url = (value, callerIds = new Set(), requestVenue = {}) => {
    discover(value);
    const parsed = new URL(value);
    parsed.username = '';
    parsed.password = '';
    if (venue === 'lighter') {
      for (const [name, item] of [...parsed.searchParams]) {
        const mapped = name === 'value' && parsed.pathname === '/api/v1/account' && parsed.searchParams.get('by') === 'index'
          ? identity(item, 'account_index') : identity(item, name);
        if (mapped !== item) parsed.searchParams.set(name, mapped);
      }
    }
    return replace(canonicalUrl(requestVenue.requestUrl?.(parsed.href) ?? parsed.href, callerIds, requestVenue));
  };
  const binary = (bytes) => {
    const input = Buffer.from(bytes);
    const formats = input[0] === 0x1f && input[1] === 0x8b
      ? [[gunzipSync, gzipSync]] : [[(value) => value, (value) => value], [inflateSync, deflateSync], [inflateRawSync, deflateRawSync]];
    for (const [decode, encode] of formats) {
      try {
        const text = new TextDecoder('utf-8', { fatal: true }).decode(decode(input));
        if (/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/.test(text)) continue;
        return Buffer.from(encode(Buffer.from(redact(text)))).toString('base64');
      } catch { /* Try a supported compressed text frame. */ }
    }
    // Persisting undecodable bytes could leak credentials. Never guess here.
    throw new Error('Live-smoke tape cannot safely redact this binary frame');
  };
  return { redact, headers, url, binary };
}

/**
 * Return transport, command, records, epoch, redact, markReady, seal and failure.
 * write receives sanitized JSON
 * objects, not strings. command records the ordinary Tape command before run.
 */
export function createPrivateRecorder(live, { venue = {}, options = {}, secrets = {}, write = () => {}, epoch = live.now(), onFailure = () => {} } = {}) {
  const replayCredentials = {
    ...venue.options, ...venue.replayCredentials,
    ...(venue.rust === 'polymarket' ? { ...POLYMARKET_REPLAY, apiKey: POLYMARKET_REPLAY.polymarket.apiKey,
      secret: POLYMARKET_REPLAY.polymarket.apiSecret, passphrase: POLYMARKET_REPLAY.polymarket.apiPassphrase } : {}),
    ...(venue.rust === 'paradex' ? Object.fromEntries(['token', 'jwt_token', 'jwtToken', 'jwt', 'bearer', 'auth'].map((name) => [name, 'replay-paradex-jwt'])) : {}),
    ...(venue.rust === 'asterdex' ? { asterdex: { userAddress: ASTER_USER, signerAddress: ASTER_SIGNER, privateKey: ASTER_PRIVATE_KEY } } : {}),
  };
  const scrub = createSmokeRedactor(secrets, replayCredentials, { venue: venue.rust });
  const records = [];
  const ordinals = new Map();
  const callerIds = new Set();
  let commandIndex = 0;
  let ready = false;
  let sealed = false;
  let failure;
  const failed = () => {
    if (!failure) {
      failure = new Error('Live-smoke tape recording failed');
      try { onFailure(failure); } catch { /* Failure reporting must never throw in a transport callback. */ }
    }
  };
  const capture = (fn) => { if (sealed) return; try { return fn(); } catch { failed(); return undefined; } };
  const emit = (record) => {
    if (sealed) return;
    const safe = scrub.redact({ t: Math.max(0, live.now() - epoch), ...record });
    try { write(safe); } catch { failed(); throw new Error('Live-smoke tape write failed'); }
    records.push(safe);
    return safe;
  };
  emit({ type: 'meta', epoch, private: true, liveSmoke: true, venue: typeof venue === 'string' ? venue : venue.rust });
  emit({ type: 'config', private: true, options: scrub.redact(options) });
  const transport = {
    ...live,
    connect(url, handlers) {
      const safeUrl = scrub.url(url, callerIds, venue);
      const socket = ordinals.get(safeUrl) ?? 0;
      ordinals.set(safeUrl, socket + 1);
      scrub.headers(handlers.headers);
      let connection;
      try { connection = live.connect(url, {
        ...handlers,
        onOpen: () => { capture(() => emit({ type: 'open', url: safeUrl, socket, ...(!ready ? { bootstrap: true } : {}) })); handlers.onOpen(); },
        onMessage: (text) => { capture(() => emit({ type: 'ws', url: safeUrl, socket, ...(!ready ? { bootstrap: true } : {}), text: scrub.redact(text) })); handlers.onMessage(text); },
        onBinary: (bytes) => { capture(() => emit({ type: 'ws', url: safeUrl, socket, ...(!ready ? { bootstrap: true } : {}), binary: scrub.binary(bytes) })); handlers.onBinary?.(bytes); },
        onClose: (code) => { capture(() => emit({ type: 'close', url: safeUrl, socket, code, ...(!ready ? { bootstrap: true } : {}) })); handlers.onClose(code); },
      }); } catch { throw new Error('Live-smoke transport connection failed'); }
      return {
        send(text) {
          capture(() => {
            // Outbound auth is audited from freshly generated replay traffic.
            // Only explicit protocol requests need their reply correlation ID.
            const safeText = scrub.redact(text);
            let message;
            try { message = JSON.parse(text); } catch { return; }
            if (venue.socketRequest?.(message)) emit({ type: 'ws-send', url: safeUrl, socket, text: safeText });
          });
          connection.send(text);
        },
        close() { connection.close(); },
      };
    },
    async request(request) {
      const stage = commandIndex;
      // Discover request/header secrets before any response echoes are persisted.
      scrub.headers(request.headers);
      const safeUrl = scrub.url(request.url, callerIds, venue);
      const requestBody = request.body === undefined ? undefined : scrub.redact(textOf(request.body));
      let response;
      try { response = await live.request(request); }
      catch { throw new Error('Live-smoke transport request failed'); }
      const headers = scrub.headers(response.headers);
      const captured = (body) => emit({ type: 'http', method: request.method, url: safeUrl,
        ...(requestBody === undefined ? {} : { requestBody }), status: response.status, headers,
        body: scrub.redact(body), afterCommand: stage });
      if (response.body instanceof Uint8Array || response.body instanceof ArrayBuffer) {
        captured(textOf(response.body));
        return response;
      }
      return { ...response, body: (async function* () {
        const decoder = new TextDecoder('utf-8', { fatal: true });
        let body = '';
        try {
          for await (const chunk of response.body) { body += decoder.decode(chunk, { stream: true }); yield chunk; }
          captured(body + decoder.decode());
        } catch { throw new Error('Live-smoke transport response recording failed'); }
      })() };
    },
  };
  const command = (frame, run) => {
    const collect = (value) => {
      if (!value || typeof value !== 'object') return;
      if (typeof value.clientOrderId === 'string') callerIds.add(value.clientOrderId);
      for (const item of Object.values(value)) collect(item);
    };
    collect(frame);
    commandIndex++;
    emit({ type: 'command', ...frame });
    return run(frame);
  };
  return { transport, command, records, epoch, seal() { sealed = true; }, markReady() { ready = true; }, get failure() { return failure; }, redact: scrub.redact };
}
