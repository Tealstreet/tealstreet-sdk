// Independent tape declarations for documented KuCoin TS read gaps. The observed
// Rust answer never supplies an expected request or an alias.
import { requestKey } from './requests.mjs';

const ORIGIN = 'https://api-futures.kucoin.com';
const fail = (message) => { throw new Error(`KuCoin read contract: ${message}`); };
const keyOf = (url) => requestKey('GET', url);
const stageOf = (record) => JSON.stringify([record.afterCommand ?? 0,
  record.availableAt ?? record.effectiveAt ?? record.availableAfter ?? 0]);
const read = (record) => {
  if (record.type !== 'http' || record.method !== 'GET' || record.status !== 200
    || record.requestBody !== undefined) fail('declarations require successful bodyless GET records');
  const url = new URL(record.url);
  if (url.origin !== ORIGIN || url.hash || url.username || url.password) fail('unexpected read origin');
  return url;
};
const dataOf = (record) => {
  const body = JSON.parse(record.body);
  if (body.code !== '200000') fail('unsuccessful venue response');
  return body.data;
};
const exact = (url, path, params) => {
  const expected = new URL(path, ORIGIN);
  for (const [key, value] of Object.entries(params)) expected.searchParams.set(key, String(value));
  if (url.searchParams.size !== Object.keys(params).length || keyOf(url.href) !== keyOf(expected.href)) fail('unexpected read parameters');
};

function kucoinRequestContract(records) {
  const commands = records.filter((record) => record.type === 'command');
  const markets = records.find((record) => record.type === 'http' && record.url === ORIGIN + '/api/v1/contracts/active');
  const symbolOf = (symbol) => {
    const market = markets && dataOf(markets).find((market) =>
      `${market.baseCurrency === 'XBT' ? 'BTC' : market.baseCurrency}${market.quoteCurrency}` === symbol);
    if (!market) fail('unknown command symbol');
    return market.symbol;
  };
  const required = new Set();
  const aliases = new Map();
  for (const record of records) {
    if (record.nativeRead === undefined && record.sameReadAs === undefined) continue;
    const url = read(record);
    if (record.nativeRead !== undefined && record.sameReadAs !== undefined) fail('ambiguous declaration');
    if (record.sameReadAs !== undefined) {
      if (typeof record.sameReadAs !== 'string') fail('invalid alias');
      const targetUrl = new URL(record.sameReadAs);
      if (!['/api/v1/orders', '/api/v1/stopOrders'].includes(url.pathname)) fail('invalid alias path');
      exact(url, url.pathname, { status: 'active' });
      if (targetUrl.origin !== url.origin || targetUrl.pathname !== url.pathname) fail('alias origin/path differs');
      exact(targetUrl, url.pathname, { status: 'active', currentPage: 1, pageSize: 100 });
      const targets = records.filter((other) => other.type === 'http' && other.url === record.sameReadAs && stageOf(other) === stageOf(record));
      if (targets.length !== 1) fail('alias requires one target at the same stage');
      const target = targets[0];
      read(target);
      if (target.nativeRead !== undefined || target.sameReadAs !== undefined || target.body !== record.body) fail('alias response differs');
      const page = dataOf(record);
      if (page.currentPage !== 1 || page.pageSize !== 100 || page.totalPage !== 1
        || !Array.isArray(page.items) || page.totalNum !== page.items.length || page.items.length > 50) fail('alias exceeds TS first-page coverage');
      const sourceKey = keyOf(record.url), targetKey = keyOf(target.url);
      if (aliases.has(sourceKey) && aliases.get(sourceKey) !== targetKey) fail('conflicting alias');
      aliases.set(sourceKey, targetKey);
      continue;
    }
    const declaration = record.nativeRead;
    if (!declaration || typeof declaration !== 'object' || Array.isArray(declaration)
      || Object.keys(declaration).some((key) => !['reason', 'command'].includes(key))) fail('invalid native read metadata');
    const command = commands[declaration.command - 1];
    switch (declaration.reason) {
      case 'query': {
        if (!Number.isInteger(declaration.command) || record.afterCommand !== declaration.command || !command || !Array.isArray(command.expected)
          || !['fetchFills', 'fetchOrderHistory', 'fetchIncome', 'fetchPublicTrades'].includes(command.op)) fail('query needs an independently expected query command');
        if (command.op === 'fetchPublicTrades') {
          exact(url, '/api/v1/trade/history', { symbol: symbolOf(command.symbol) });
          break;
        }
        if (![command.from, command.to].every(Number.isSafeInteger) || command.from > command.to) fail('invalid query range');
        const range = { startAt: command.from, endAt: command.to };
        if (command.op === 'fetchFills' || command.op === 'fetchOrderHistory') {
          exact(url, command.op === 'fetchFills' ? '/api/v1/fills' : '/api/v1/orders', {
            symbol: symbolOf(command.symbol), ...(command.op === 'fetchOrderHistory' ? { status: 'done' } : {}),
            ...range, currentPage: 1, pageSize: 50,
          });
        } else if (command.incomeType === 'FUNDING_FEE') {
          exact(url, '/api/v1/funding-history', { symbol: symbolOf(command.symbol), ...range, reverse: true, forward: false, maxCount: 50 });
        } else {
          if (command.incomeType !== undefined || command.symbol !== undefined) fail('unsupported transaction-history declaration');
          exact(url, '/api/v1/transaction-history', { ...range, currency: 'USDT', forward: false, maxCount: 50 });
        }
        break;
      }
      case 'cancel-proof': {
        if (!Number.isInteger(declaration.command) || command?.op !== 'amendOrder'
          || record.afterCommand !== declaration.command || typeof command.id !== 'string') fail('cancel proof needs its amend command stage');
        const data = dataOf(record);
        exact(url, `/api/v1/orders/${encodeURIComponent(command.id)}`, { symbol: data.symbol });
        const placed = records.some((other) => other.type === 'http' && other.method === 'POST'
          && other.status === 200 && new URL(other.url).pathname === '/api/v1/orders'
          && dataOf(other).orderId === command.id && JSON.parse(other.requestBody).symbol === data.symbol);
        if (!placed || data.id !== command.id || data.isActive !== false || data.cancelExist !== true
          || !records.some((other) => other.type === 'http' && other.method === 'DELETE' && other.url === record.url
            && other.status === 200 && dataOf(other).cancelledOrderIds?.includes(command.id))) fail('cancel proof does not bind the canceled order');
        break;
      }
      case 'position-mode': {
        if (declaration.command !== undefined || stageOf(record) !== '[0,0]') fail('position mode is an authoritative startup read');
        exact(url, '/api/v2/position/getPositionMode', {});
        if (![0, 1].includes(dataOf(record).positionMode)) fail('invalid authoritative position mode');
        break;
      }
      default: fail('unknown native read reason');
    }
    required.add(keyOf(record.url));
  }
  const normalize = (requests) => [...new Set(requests.map((key) => aliases.get(key) ?? key))].sort();
  return { required, normalize };
}

export function expectedKucoinRequests(records, requests) {
  const { required, normalize } = kucoinRequestContract(records);
  return normalize([...requests, ...required]);
}

export function applyKucoinRequestContracts(records, tsRequests, rustRequests) {
  const { required, normalize } = kucoinRequestContract(records);
  for (const key of required) if (!rustRequests.includes(key)) fail(`required native GET was not sent: ${key}`);
  return { tsRequests: normalize([...tsRequests, ...required]), rustRequests: normalize(rustRequests) };
}
