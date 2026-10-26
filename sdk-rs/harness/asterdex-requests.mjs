// Independent request oracle: Ethereum primitives come from ethers, not the engine signer.
import { createHmac } from 'node:crypto';
import { createRequire } from 'node:module';

const require = createRequire(import.meta.url);
let crypto;
export const initializeAsterCrypto = () => crypto ??= require('ethers');

// Public documentation demonstration identities; never a funded account.
export const ASTER_PRIVATE_KEY = '0x4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1';
export const ASTER_USER = '0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e';
export const ASTER_SIGNER = '0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0';
const ZERO = '0x0000000000000000000000000000000000000000';
const PUBLIC = /^\/fapi\/v[13]\/(?:time|exchangeInfo|ticker(?:\/.*)?|openInterest|depth|klines|trades|aggTrades|ping)$/;

export const asterRequestUrl = (url) => {
  const parsed = new URL(url);
  if (parsed.pathname.startsWith('/fapi/v3/')) parsed.searchParams.delete('nonce');
  return parsed.href;
};

const fresh = (value, now, scale = 1n) =>
  /^\d+$/.test(value ?? '') &&
  BigInt(value) > 0n &&
  BigInt(value) / scale >= BigInt(Math.floor(now - 60000)) &&
  BigInt(value) / scale <= BigInt(Math.ceil(now + 60000));

const validTiming = (value, now) =>
  /^\d+$/.test(value ?? '') &&
  Number.isSafeInteger(Number(value)) &&
  Number(value) > 0 &&
  Number(value) >= now - 20000 &&
  Number(value) < now + 1000;

export const createAsterRequestValidator = (options = {}) => {
  const { AbiCoder, getAddress, getBytes, hashMessage, keccak256, recoverAddress, TypedDataEncoder, Wallet } = initializeAsterCrypto();
  const nonces = new Map();
  return ({ method, url, headers = {}, body, now = Date.now() }) => {
    const parsed = new URL(url);
    if (!['fapi.asterdex.com', 'fapi.asterdex-testnet.com'].includes(parsed.hostname))
      throw new Error('Aster request must use its native upstream URL');
    if (PUBLIC.test(parsed.pathname)) return;
    if (options.publicDataOnly && parsed.pathname.endsWith('/leverageBracket') && !parsed.searchParams.has('signature'))
      return;
    if (body !== undefined && body !== null && body !== '')
      throw new Error('Aster signed request must use query parameters without a body');
    const params = parsed.searchParams;
    const fields = [...params.keys()];
    if (fields.length !== new Set(fields).size) throw new Error('Aster signing query has duplicate fields');
    const header = Object.fromEntries(
      (Array.isArray(headers) ? headers : Object.entries(headers)).map(([key, value]) => [
        key.toLowerCase(),
        String(value),
      ]),
    );
    const read = method === 'GET' && Boolean(options.asterdex?.readKey ?? options.secondary_key);
    const key = read ? (options.asterdex?.readKey ?? options.secondary_key) : (options.key ?? 'replay-key');
    const v3 = parsed.pathname.startsWith('/fapi/v3/');
    const accountSecret = read
      ? (options.asterdex?.readSecret ?? options.secondary_secret)
      : (options.secret ?? 'replay-secret');
    const secret = v3 ? (options.asterdex?.privateKey ?? options.auth?.privateKey ?? accountSecret) : accountSecret;
    if (header['x-mbx-apikey'] !== key) throw new Error('Aster request has the wrong API wallet/key');
    const signature = params.get('signature');
    if (!signature) throw new Error('Aster signed request is missing its signature');
    const signed = [...params].filter(([name]) => name !== 'signature');
    if (!v3) {
      if (params.get('recvWindow') !== '20000' || !validTiming(params.get('timestamp'), now))
        throw new Error('Aster V1 timestamp/window is invalid');
      const payload = signed.map(([name, value]) => `${name}=${value}`).join('&');
      if (createHmac('sha256', secret).update(payload).digest('hex') !== signature)
        throw new Error('Aster V1 HMAC mismatch');
      return;
    }
    const user = getAddress(
      options.asterdex?.userAddress ?? options.auth?.userAddress ?? options.applicationId ?? ASTER_USER,
    );
    const signer = getAddress(
      options.asterdex?.signerAddress ?? options.auth?.signerAddress ?? (read ? key : (options.key ?? ASTER_SIGNER)),
    );
    if (params.get('user') !== user || params.get('signer') !== signer)
      throw new Error('Aster V3 owner/signer identity mismatch');
    const nonce = params.get('nonce');
    if (!fresh(nonce, now, 1000n)) throw new Error('Aster V3 nonce is not fresh microseconds');
    const scope = signer.toLowerCase();
    if (nonces.get(scope)?.has(nonce)) throw new Error('Aster V3 signer nonce was reused');
    const code =
      (method === 'POST' && parsed.pathname.endsWith('/order')) ||
      (method === 'GET' && /\/(builder|agent)$/.test(parsed.pathname));
    let digest;
    if (code) {
      const testnet = parsed.hostname === 'fapi.asterdex-testnet.com';
      if (
        params.get('asterChain') !== (testnet ? 'Testnet' : 'Mainnet') ||
        params.has('timestamp') ||
        params.has('recvWindow')
      )
        throw new Error('Aster Code signing fields/domain mismatch');
      const rawQuery = parsed.search.slice(1);
      const suffix = `&signature=${signature}`;
      if (!rawQuery.endsWith(suffix)) throw new Error('Aster Code signature must follow the exact signed query');
      digest = TypedDataEncoder.hash(
        { name: 'AsterSignTransaction', version: '1', chainId: testnet ? 714 : 1666, verifyingContract: ZERO },
        { Message: [{ name: 'msg', type: 'string' }] },
        { msg: rawQuery.slice(0, -suffix.length) },
      );
    } else {
      if (
        params.get('recvWindow') !== '20000' ||
        !validTiming(params.get('timestamp'), now) ||
        params.has('asterChain')
      )
        throw new Error('Aster V3 timestamp/window is invalid');
      const payload = Object.fromEntries(
        signed.filter(([name]) => !['user', 'signer', 'nonce'].includes(name)).sort(([a], [b]) => a.localeCompare(b)),
      );
      const encoded = AbiCoder.defaultAbiCoder().encode(
        ['string', 'address', 'address', 'uint256'],
        [JSON.stringify(payload), user, signer, BigInt(nonce)],
      );
      digest = hashMessage(getBytes(keccak256(encoded)));
    }
    const recovered = recoverAddress(digest, signature);
    if (recovered !== new Wallet(secret).address || recovered !== signer)
      throw new Error('Aster V3 recoverable signature mismatch');
    if (!nonces.has(scope)) nonces.set(scope, new Set());
    nonces.get(scope).add(nonce);
  };
};

export const asterSocketAudit = (_impl, { isPrivate, books }) => {
  const states = new Map();
  const failures = [];
  const state = (url) => {
    const parsed = new URL(url);
    const key = parsed.origin + parsed.pathname;
    if (!states.has(key))
      states.set(key, { topics: new Set(), ping: false, private: parsed.pathname.startsWith('/ws/') });
    return states.get(key);
  };
  return {
    sent(url, text) {
      const current = state(url);
      const message = JSON.parse(text);
      if (message.method === 'LIST_SUBSCRIPTIONS') {
        if (message.id !== (current.private ? 42 : -1)) failures.push('Aster ping has the wrong reserved identity');
        current.ping = true;
      } else if (['SUBSCRIBE', 'UNSUBSCRIBE'].includes(message.method)) {
        if (current.private || !Number.isSafeInteger(message.id) || message.id <= 0 || !Array.isArray(message.params))
          failures.push('Invalid Aster public subscription');
        for (const topic of message.params ?? []) {
          if (message.method === 'SUBSCRIBE') current.topics.add(topic);
          else current.topics.delete(topic);
        }
      } else failures.push('Unexpected Aster outbound socket method');
    },
    received(url, text) {
      const current = state(url);
      const message = JSON.parse(text);
      if (message.stream && !current.topics.has(message.stream))
        failures.push(`Aster data arrived without subscription: ${message.stream}`);
      if (current.private && !current.ping) failures.push('Aster private data arrived before socket initialization');
    },
    ready() {
      return (
        [...states.values()].some(
          (s) =>
            !s.private &&
            s.ping &&
            [
              '!ticker@arr',
              '!markPrice@arr@1s',
              ...books.map((symbol) => `${symbol.toLowerCase()}@depth20@100ms`),
            ].every((topic) => s.topics.has(topic)),
        ) &&
        (!isPrivate || [...states.values()].some((s) => s.private && s.ping))
      );
    },
    errors() {
      if (!this.ready()) failures.push('Aster socket initialization/subscriptions incomplete');
      return [...new Set(failures)];
    },
  };
};
