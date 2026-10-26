// A private recorder for EngineHost's transport interface. Only sanitized copies
// enter the tape; the engine and the real transport keep the original bytes.
import { gzipSync, gunzipSync, deflateSync, inflateSync, deflateRawSync, inflateRawSync } from 'node:zlib';
import { canonicalUrl, isWriteRequest } from './requests.mjs';
import { TAPE_FIELDS, tapeHeaders } from './tape-fields.mjs';
import { POLYMARKET_REPLAY } from './polymarket.mjs';
import { ASTER_PRIVATE_KEY, ASTER_USER, ASTER_SIGNER } from './asterdex-requests.mjs';

const normalized = (name) => String(name).split('.').at(-1).toLowerCase().replace(/[-_]/g, '');
const credentialNames = new Set(['key', 'apikey', 'readkey', 'readsecret', 'secondarykey', 'secondarysecret', 'secret', 'apisecret', 'privatekey', 'passphrase', 'password', 'authorization', 'cookie', 'setcookie']);
const sessionNames = new Set(['session', 'sessionid', 'token', 'accesstoken', 'refreshtoken', 'authtoken', 'sessiontoken', 'sessionkey', 'listenkey', 'jwt', 'jwttoken', 'auth', 'bearer']);
const signatureNames = new Set(['signature', 'sig', 'sign', 'signedchallenge', 'authent']);
// Header signatures conventionally end in a delimited SIGN (ACCESS-SIGN,
// KC-API-PARTNER-SIGN, X-CH-SIGN); matching only SIGNATURE misses these.
const signatureName = (name) => signatureNames.has(normalized(name)) || /(?:signature|apisign)$/.test(normalized(name)) || /(?:^|[-_])sign$/i.test(name);
const sensitive = (name) => credentialNames.has(normalized(name)) || sessionNames.has(normalized(name)) || signatureName(name) || /(?:apikey|apisecret|accesskey|orderlykey|passphrase|accesstoken|authtoken|refreshtoken|authorization)$/.test(normalized(name));
const parseJson = (text) => JSON.parse(text, (_key, value, context) =>
  typeof value === 'number' && Number.isInteger(value) && !Number.isSafeInteger(value)
    ? JSON.rawJSON(context.source) : value);
const textOf = (value) => {
  if (value instanceof URLSearchParams) return value.toString();
  if (value instanceof ArrayBuffer) return new TextDecoder('utf-8', { fatal: true }).decode(new Uint8Array(value));
  if (ArrayBuffer.isView(value)) return new TextDecoder('utf-8', { fatal: true }).decode(value);
  return typeof value === 'string' ? value : JSON.stringify(value);
};

class UnrecordableSecret extends Error {
  constructor() { super('Live-smoke credential material is too short to record safely'); }
}

/** Known secrets plus discovered auth/session fields are scrubbed before write. */
export function createSmokeRedactor(secrets = {}, replayCredentials = {}, { venue } = {}) {
  const aliases = new Map();
  const configuredNames = new Set(Object.keys(secrets).map(normalized));
  let next = 0;
  const remember = (name, value) => {
    if (typeof value !== 'string' || !value) return;
    // Short public account/index metadata is allowed; short authentication
    // material cannot be scrubbed from arbitrary echoes without corrupting IDs.
    if (sensitive(name) && value.length < 4) throw new UnrecordableSecret();
    if (aliases.has(value) || [...aliases.values()].includes(value)) return;
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
          : signatureName(name) ? '<redacted-signature>'
            : `live-smoke-${sessionNames.has(key) ? 'session' : 'secret'}-${++next}`;
    aliases.set(value, alias);
  };
  const allowed = (name) => TAPE_FIELDS.has(name) || sensitive(name) || configuredNames.has(normalized(name));
  const unknown = (value) => {
    if (typeof value === 'string') remember('secret', value);
    else if (value && typeof value === 'object') for (const item of Object.values(value)) unknown(item);
  };
  const discover = (value, name = '') => {
    if (typeof value === 'string') {
      if (sensitive(name) || configuredNames.has(normalized(name))) { remember(name, value); return; }
      if (['binary', 'base64'].includes(name)) return;
      try {
        const url = new URL(value);
        if (url.username) remember('key', decodeURIComponent(url.username));
        if (url.password) remember('password', decodeURIComponent(url.password));
        if (!['https:', 'http:', 'wss:', 'ws:'].includes(url.protocol)) throw new Error('not a protocol URL');
        for (const [key, item] of url.searchParams) {
          if (sensitive(key)) remember(key, item);
          else if (!allowed(key)) { remember('secret', key); unknown(item); }
        }
        return;
      } catch (error) { if (error instanceof UnrecordableSecret) throw error; /* Plain strings and non-URL payloads. */ }
      if (/^[\s]*[\[{\"]/.test(value)) {
        try { discover(parseJson(value)); } catch (error) { if (error instanceof UnrecordableSecret) throw error; /* Plain text. */ }
      } else if (/(?:^|&)[A-Za-z_%-][^=&]*=/.test(value)) {
        for (const [key, item] of new URLSearchParams(value)) {
          if (allowed(key)) discover(item, key);
          else { remember('secret', key); unknown(item); }
        }
      }
    } else if (Array.isArray(value)) {
      for (const item of value) discover(item);
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
      for (const [key, item] of Object.entries(value)) {
        if (allowed(key)) discover(item, key);
        else { remember('secret', key); unknown(item); }
      }
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
      const formEncoded = new URLSearchParams({ value: secret }).toString().slice('value='.length);
      const forms = new Set([secret, encoded, formEncoded, formEncoded.replace(/%[\dA-F]{2}/g, (part) => part.toLowerCase()), encoded.replace(/%[\dA-F]{2}/g, (part) => part.toLowerCase()),
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
      if (['binary', 'base64'].includes(name)) return replace(value);
      if (/^[\s]*[\[{\"]/.test(value)) {
        try { return JSON.stringify(visit(parseJson(value))); } catch { /* Plain text. */ }
      }
      try {
        const parsed = new URL(value);
        if (['https:', 'http:', 'wss:', 'ws:'].includes(parsed.protocol)) return url(value, undefined, undefined, false);
      } catch { /* Plain protocol strings. */ }
      if (/^[A-Za-z_%][\w%.-]*=/.test(value)) {
        return new URLSearchParams([...new URLSearchParams(value)].filter(([key]) => allowed(key)).map(([key, item]) => [key, visit(item, key, [...path, key])])).toString();
      }
      return replace(value);
    }
    if (JSON.isRawJSON(value)) return value;
    if (Array.isArray(value)) return value.map((item) => visit(item, '', path));
    if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value)
      .filter(([key]) => allowed(key)).map(([key, item]) => [replace(key), visit(item, key, [...path, key])]));
    return value;
  };
  const redact = (value) => { discover(value); return visit(value); };
  const headers = (value = '') => {
    const entries = typeof value === 'string'
      ? value.split(/\r?\n/).filter(Boolean).map((line) => { const at = line.indexOf(':'); return [line.slice(0, at), line.slice(at + 1).trim()]; })
      : value instanceof Headers ? [...value] : Array.isArray(value) ? value : Object.entries(value);
    const safe = (name) => /^(?:content-type|content-encoding|date|retry-after|x-request-id|x-ratelimit-[a-z-]+|ratelimit-[a-z-]+|x-mbx-used-weight(?:-\d+[smhd])?)$/i.test(name);
    for (const [name, item] of entries) {
      if (sensitive(name)) remember(name, String(item));
      else if (!safe(name)) unknown(String(item));
    }
    return tapeHeaders(entries).filter(([name]) => safe(name) || sensitive(name))
      .map(([name, item]) => `${name}: ${sensitive(name) ? aliases.get(String(item)) : replace(String(item))}`).join('\n');
  };
  const url = (value, callerIds = new Set(), requestVenue = {}, canonicalize = true) => {
    discover(value);
    const parsed = new URL(value);
    for (const [name, item] of [...parsed.searchParams]) if (!allowed(name)) {
      remember('secret', name); unknown(item); parsed.searchParams.delete(name);
    }
    parsed.username = '';
    parsed.password = '';
    if (venue === 'lighter') {
      for (const [name, item] of [...parsed.searchParams]) {
        const mapped = name === 'value' && parsed.pathname === '/api/v1/account' && parsed.searchParams.get('by') === 'index'
          ? identity(item, 'account_index') : identity(item, name);
        if (mapped !== item) parsed.searchParams.set(name, mapped);
      }
    }
    return replace(canonicalize ? canonicalUrl(requestVenue.requestUrl?.(parsed.href) ?? parsed.href, callerIds, requestVenue) : parsed.href);
  };
  const binary = (bytes) => {
    const input = Buffer.from(bytes);
    const limit = 16 * 1024 * 1024;
    if (input.byteLength > limit) throw new Error('Live-smoke binary frame exceeds limit');
    const formats = input[0] === 0x1f && input[1] === 0x8b
      ? [[gunzipSync, gzipSync]] : [[(value) => value, (value) => value], [inflateSync, deflateSync], [inflateRawSync, deflateRawSync]];
    for (const [decode, encode] of formats) {
      try {
        const text = new TextDecoder('utf-8', { fatal: true }).decode(decode(input, { maxOutputLength: limit }));
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
  const requiredRedaction = (fn) => {
    try { return fn(); } catch { failed(); throw new Error('Live-smoke tape redaction failed'); }
  };
  const emit = (record) => {
    if (sealed) return;
    const safe = scrub.redact({ t: Math.max(0, live.now() - epoch), ...record });
    try { write(safe); } catch { failed(); throw new Error('Live-smoke tape write failed'); }
    records.push(safe);
    return safe;
  };
  emit({ type: 'meta', epoch, private: true, liveSmoke: true, venue: typeof venue === 'string' ? venue : venue.rust });
  emit({ type: 'config', private: true, options: scrub.redact(options) });
  const prepareRecording = (request, stage) => {
    const writeRequest = venue.isWrite?.(request.method, request.url) ?? isWriteRequest(request.method, request.url);
    // Discover request/header secrets before any response echoes are persisted.
    requiredRedaction(() => scrub.headers(request.headers));
    const safeUrl = requiredRedaction(() => scrub.url(request.url, callerIds, venue));
    const requestBody = request.body === undefined ? undefined : requiredRedaction(() => scrub.redact(textOf(request.body)));
    return { method: request.method, stage, writeRequest, safeUrl, requestBody };
  };
  const recordResponse = async (pending, { method, stage, writeRequest, safeUrl, requestBody }) => {
    let response;
    try { response = await pending; }
    catch { throw new Error('Live-smoke transport request failed'); }
    const headers = requiredRedaction(() => scrub.headers(response.headers));
    const captured = (body) => {
      try { return emit({ type: 'http', method, url: safeUrl,
        ...(requestBody === undefined ? {} : { requestBody }), status: response.status, headers,
        // A delayed read first exposes its state at completion. Writes remain
        // attached to the command that issued them so replay can await them.
        body: scrub.redact(body), afterCommand: writeRequest ? stage : commandIndex }); }
      catch { failed(); throw new Error('Live-smoke transport response recording failed'); }
    };
    if (response.body instanceof Uint8Array || response.body instanceof ArrayBuffer) {
      captured(requiredRedaction(() => textOf(response.body)));
      return response;
    }
    return { ...response, body: (async function* () {
      const decoder = new TextDecoder('utf-8', { fatal: true });
      let body = '';
      try {
        for await (const chunk of response.body) { body += decoder.decode(chunk, { stream: true }); yield chunk; }
        captured(body + decoder.decode());
      } catch { failed(); throw new Error('Live-smoke transport response recording failed'); }
    })() };
  };
  const preparationFailure = (error) => {
    const safe = new Error('Live-smoke transport preparation failed');
    if (['UNSUPPORTED', 'TIMED_OUT', 'STALE', 'INVALID', 'NETWORK'].some((code) => error?.code === `HTTP_PREPARATION_${code}`)) safe.code = error.code;
    return safe;
  };
  const transport = {
    ...live,
    connect(url, handlers) {
      const safeUrl = requiredRedaction(() => scrub.url(url, callerIds, venue));
      const socket = ordinals.get(safeUrl) ?? 0;
      ordinals.set(safeUrl, socket + 1);
      requiredRedaction(() => scrub.headers(handlers.headers));
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
    prepareHttp: typeof live.prepareHttp !== 'function' ? undefined : async (request) => {
      const stage = commandIndex;
      let prepared;
      try { prepared = await live.prepareHttp(request); }
      catch (error) { throw preparationFailure(error); }
      return {
        expiresAt: prepared.expiresAt,
        cancel: () => prepared.cancel?.(),
        bind(packet) {
          try {
            const capture = prepareRecording(packet, stage);
            const bound = prepared.bind(packet);
            return {
              ready: () => bound.ready(),
              cancel: () => bound.cancel?.(),
              dispatch() {
                let response;
                try { response = bound.dispatch(); }
                catch { throw new Error('Live-smoke transport request failed'); }
                return recordResponse(response, capture);
              },
            };
          } catch (error) {
            try { prepared.cancel?.(); } catch { /* Preserve the refusal and cleanup. */ }
            throw preparationFailure(error);
          }
        },
      };
    },
    async request(request) {
      const capture = prepareRecording(request, commandIndex);
      let response;
      try { response = live.request(request); }
      catch { throw new Error('Live-smoke transport request failed'); }
      return recordResponse(response, capture);
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
