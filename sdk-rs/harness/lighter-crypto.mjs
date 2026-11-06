// Independent replay verifier: the checked-in venue Go-WASM, never the Rust signer.
import { readFileSync } from 'node:fs';
import { parseRequestJson } from './request-json.mjs';
let crypto;
const result = (value) => {
  if (typeof value !== 'string') throw new Error(`Lighter oracle crypto failed: ${JSON.stringify(value)}`);
  return value.replace(/^0x/, '');
};
export async function initializeLighterCrypto(impl = 'rust') {
  if (crypto) return crypto;
  if (impl === 'ts') {
    const original = await import('../../packages/poseidon-crypto/src/index.ts');
    await original.initWasm();
    crypto = original.getCryptoApi();
  } else {
    await import('../../packages/poseidon-crypto/src/wasm/wasm_exec.js');
    const go = new globalThis.Go();
    const { instance } = await WebAssembly.instantiate(readFileSync(new URL('../../packages/poseidon-crypto/src/wasm/poseidon_crypto.wasm', import.meta.url)), go.importObject);
    void go.run(instance);
    if (typeof globalThis.schnorrVerify !== 'function') throw new Error('Lighter Go-WASM did not initialize');
    crypto = Object.fromEntries(['poseidon2Hash', 'poseidon2HashFp5', 'poseidon2HashTwoToOne', 'schnorrPkFromSk', 'schnorrVerify', 'schnorrSign'].map((key) => [key, (...args) => globalThis[key](...args)]));
  }
  return crypto;
}
const integer = (value, max = BigInt(Number.MAX_SAFE_INTEGER)) => {
  if (!(typeof value === 'number' && Number.isSafeInteger(value)) && typeof value !== 'bigint') throw new Error('Lighter wire integer required');
  const n = BigInt(value);
  if (n < 0n || n > max) throw new Error('Lighter wire integer out of range');
  return n;
};
const words = (...values) => Buffer.concat(values.map((value) => {
  const bytes = Buffer.alloc(8);
  bytes.writeBigUInt64LE(integer(value, (1n << 63n) - 1n));
  return bytes;
})).toString('hex');
const uint32 = (value) => Number(integer(value, 0xffff_ffffn));
const orderWords = (order, now) => {
  integer(order.MarketIndex, 32767n);
  if (order.MarketIndex === 255) throw new Error("Lighter market sentinel is not tradable");
  integer(order.ClientOrderIndex, (1n << 48n) - 1n);
  integer(order.BaseAmount, (1n << 48n) - 1n);
  uint32(order.Price); uint32(order.TriggerPrice);
  integer(order.IsAsk, 1n); integer(order.Type, 5n); integer(order.TimeInForce, 2n); integer(order.ReduceOnly, 1n);
  integer(order.OrderExpiry);
  if (order.TimeInForce === 0 && order.Type < 2) {
    if (order.OrderExpiry !== 0) throw new Error('Lighter IOC order expiry must be zero');
  } else if (Math.abs(order.OrderExpiry - now - 28 * 86400_000) > 60_000) throw new Error('Lighter order expiry must be fresh milliseconds');
  return words(order.MarketIndex, order.ClientOrderIndex, order.BaseAmount, order.Price, order.IsAsk, order.Type, order.TimeInForce, order.ReduceOnly, order.TriggerPrice, order.OrderExpiry);
};
export const decodeLighterTransactions = (body) => {
  if (ArrayBuffer.isView(body) || body instanceof ArrayBuffer) body = new TextDecoder().decode(body);
  const form = body instanceof URLSearchParams ? body : new URLSearchParams(body);
  if (form.has('tx_types')) {
    const types = JSON.parse(form.get('tx_types'));
    const infos = JSON.parse(form.get('tx_infos'));
    if (!Array.isArray(types) || !Array.isArray(infos) || !types.length || types.length !== infos.length) throw new Error('Invalid Lighter transaction batch');
    return types.map((type, index) => ({ type, info: parseRequestJson(infos[index]) }));
  }
  return [{ type: Number(form.get('tx_type')), info: parseRequestJson(form.get('tx_info')) }];
};
// Client order indexes a write creates: one per order, grouped orders included.
const createdIds = (type, info) => type === 14 ? [String(info.ClientOrderIndex)]
  : type === 28 && Array.isArray(info.Orders) ? info.Orders.map((order) => String(order.ClientOrderIndex)) : [];
export function createLighterVerifier(options, oracle = crypto, exchangeName = 'lighter') {
  if (!oracle) throw new Error('Initialize the independent Lighter oracle first');
  const account = String(options.applicationId);
  const apiKey = Number(options.apiKeyIndex);
  const publicKey = result(oracle.schnorrPkFromSk(options.secret));
  const generated = new Set();
  const commandIds = new Map();
  const auth = (token, now = Date.now()) => {
    if (typeof token !== 'string') return false;
    const parts = token.split(':');
    if (parts.length !== 4 || !/^\d+$/.test(parts[0]) || parts[1] !== account || parts[2] !== String(apiKey) || !/^[a-f\d]{160}$/i.test(parts[3])) return false;
    const deadline = Number(parts[0]);
    if (!Number.isSafeInteger(deadline) || deadline < now / 1000 || deadline > now / 1000 + 3600) return false;
    const message = Buffer.from(parts.slice(0,3).join(':'));
    const padded = Buffer.alloc(Math.ceil(message.length / 8) * 8); message.copy(padded);
    return oracle.schnorrVerify(padded.toString('hex'), publicKey, parts[3]) === true;
  };
  const verifyTransaction = ({type, info}, now) => {
    if (String(info.AccountIndex) !== account || info.ApiKeyIndex !== apiKey || !Number.isSafeInteger(info.ExpiredAt)
      || Math.abs(info.ExpiredAt - now - 600_000) > 60_000) return false;
    integer(info.Nonce); integer(info.AccountIndex, (1n << 48n) - 2n);
    integer(info.ExpiredAt, (1n << 48n) - 1n); integer(type, 255n);
    let preimage = words(options.testnet ? 300 : exchangeName === 'lighter-robinhood' ? 466324 : 304, type, info.Nonce, info.ExpiredAt, info.AccountIndex, info.ApiKeyIndex);
    let ids = [];
    if (type === 14) {
      preimage += orderWords(info, now);
      ids = [String(info.ClientOrderIndex)];
    } else if (type === 28) {
      // create_grouped_orders: GroupingType, then Go's HashNToOne over each order's HashNoPad.
      const grouping = Number(integer(info.GroupingType, 3n));
      const count = grouping === 3 ? 3 : grouping === 1 || grouping === 2 ? 2 : 0;
      if (!Array.isArray(info.Orders) || !count || info.Orders.length !== count) return false;
      const hashes = info.Orders.map((order) => result(oracle.poseidon2Hash(orderWords(order, now))));
      preimage += words(grouping) + hashes.slice(1).reduce((digest, hash) => result(oracle.poseidon2HashTwoToOne(digest, hash)), hashes[0]);
      ids = info.Orders.map((order) => String(order.ClientOrderIndex));
    } else if (type === 15) {
      integer(info.MarketIndex,32767n);
      if (info.MarketIndex === 255) return false;
      if (integer(info.Index, (1n << 60n) - 1n) === 0n) return false;
      preimage += words(info.MarketIndex,info.Index);
    } else if (type === 20) {
      integer(info.MarketIndex,32767n);
      if (info.MarketIndex === 255) return false; integer(info.InitialMarginFraction,10_000n); integer(info.MarginMode,1n);
      if (!info.InitialMarginFraction) return false;
      preimage += words(info.MarketIndex,info.InitialMarginFraction,info.MarginMode);
    } else return false;
    if (new Set(ids).size !== ids.length || ids.some((id) => generated.has(id))) return false;
    if (info.L2TxAttributes !== undefined) {
      if (type !== 14 && type !== 28) return false;
      const attrs = info.L2TxAttributes;
      if (!attrs || Object.keys(attrs).sort().join(',') !== '1,2,3') return false;
      for (const value of Object.values(attrs)) integer(value);
      const attrWords = Object.entries(attrs).filter(([,value]) => value !== 0).flatMap(([kind,value]) => [Number(kind), value]);
      while (attrWords.length < 8) attrWords.push(0);
      preimage = result(oracle.poseidon2HashFp5(preimage)) + result(oracle.poseidon2HashFp5(words(...attrWords)));
    }
    if (typeof info.Sig !== 'string') return false;
    const bytes = Buffer.from(info.Sig,'base64');
    if (bytes.length !== 80 || bytes.toString('base64') !== info.Sig) return false;
    return oracle.schnorrVerify(preimage,publicKey,bytes.toString('hex')) === true;
  };
  return {
    auth,
    request({method,url,body,headers={},now=Date.now()}) {
      try {
        const parsed = new URL(url);
        const entries = Array.isArray(headers) ? headers : Object.entries(headers);
        const tokens = [...parsed.searchParams.getAll('auth'), ...entries.filter(([key]) => key.toLowerCase() === 'authorization').map(([,token]) => token)];
        if (tokens.some((token) => !auth(token,now))) return false;
        if (tokens.length && parsed.searchParams.has('account_index') && parsed.searchParams.get('account_index') !== account) return false;
        const privateRead = /\/(accountActiveOrders|accountInactiveOrders|accountLimits|accountTxs|positionFunding)$/.test(parsed.pathname)
          || (parsed.pathname.endsWith('/trades') && parsed.searchParams.has('account_index'));
        if (privateRead && !tokens.length) return false;
        if (!/\/sendTx(?:Batch)?$/.test(parsed.pathname)) return method === 'GET';
        const transactions = decodeLighterTransactions(body);
        const ids = transactions.flatMap(({type,info}) => createdIds(type,info));
        return method === 'POST' && new Set(ids).size === ids.length
          && transactions.every((tx) => verifyTransaction(tx,now));
      } catch { return false; }
    },
    subscribe(message, now=Date.now()) {
      const channel = String(message.channel ?? '');
      if (channel.startsWith('account_all_orders/')) return channel === `account_all_orders/${account}` && auth(message.auth,now);
      if (channel.startsWith('account_all/')) return channel === `account_all/${account}`;
      return true;
    },
    matched(record,{body,commandIndex=1}) {
      if (!/\/sendTx(?:Batch)?$/.test(new URL(record.url).pathname) || Number(JSON.parse(record.body).code) !== 200) return;
      const transactions = decodeLighterTransactions(body);
      const payload = JSON.parse(record.body);
      const hashes = payload.tx_hash ?? payload.txHash;
      const valid = transactions.length === 1 && typeof hashes === 'string' ? hashes.length > 0
        : Array.isArray(hashes) && hashes.length === transactions.length && hashes.every((hash) => typeof hash === 'string' && hash.length > 0);
      if (!valid) return;
      for (const {type,info} of transactions) {
        const ids = createdIds(type,info);
        for (const id of ids) {
          if (generated.has(String(id))) throw new Error('Lighter generated order index reused');
          generated.add(String(id));
          const created = commandIds.get(commandIndex) ?? [];
          created.push(String(id));
          commandIds.set(commandIndex, created);
        }
      }
    },
    // A tape frame may quote an index this replay created: "<created:command:order>".
    echoCreated(text) {
      return text.replace(/"<created:(\d+):(\d+)>"/g, (_, command, order) => {
        const id = commandIds.get(Number(command))?.[Number(order)];
        if (id === undefined) throw new Error(`Lighter tape quotes order ${order} that command ${command} never created`);
        return JSON.stringify(id);
      });
    },
    commandResult(result, command, commandIndex=1) {
      if (!['placeOrders','amendOrder'].includes(command?.op) || !result?.ok) return result;
      const normalize = (ids) => {
        const expected = commandIds.get(commandIndex) ?? [];
        if (!Array.isArray(ids) || ids.length !== expected.length || ids.some((id,index) => String(id) !== expected[index]))
          throw new Error('Lighter returned order indexes do not match this command\'s signed and acknowledged creations');
        return ids.map(() => '<generated>');
      };
      return {ok: Array.isArray(result.ok) ? normalize(result.ok) : {...result.ok,orderIds:normalize(result.ok.orderIds)}};
    },
  };
}
const stableInfo = (info) => {
  const copy = {...info};
  for (const key of ['Sig','ExpiredAt']) delete copy[key];
  if ('ClientOrderIndex' in copy) copy.ClientOrderIndex = '<generated>';
  if ('OrderExpiry' in copy) copy.OrderExpiry = copy.OrderExpiry === 0 ? 0 : '<expiry>';
  if (copy.Orders) copy.Orders = copy.Orders.map(stableInfo);
  return copy;
};
export const lighterRequestBody = (body) => {
  if (!body || (!('tx_info' in body) && !('tx_infos' in body))) return body;
  const canonical = (type, text) => ({type,info:stableInfo(parseRequestJson(text))});
  if (body.tx_infos) return JSON.parse(body.tx_types).map((type,index) => canonical(type,JSON.parse(body.tx_infos)[index]));
  return canonical(Number(body.tx_type),body.tx_info);
};
