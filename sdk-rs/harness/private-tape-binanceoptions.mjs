// Offline replay inputs come from the retained TypeScript Options mapper tests.
// No sockets, API keys or live venue requests are used by this generator.
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { runInNewContext } from 'node:vm';
import ts from 'typescript';

export function buildBinanceOptionsTape({ isPublic = false } = {}) {
  const source = readFileSync(
    new URL('../../packages/safe-cex/src/__tests__/binanceoptions.test.ts', import.meta.url),
    'utf8',
  );
  const tree = ts.createSourceFile('binanceoptions.test.ts', source, ts.ScriptTarget.Latest, true);
  const wanted = new Set([
    'EXCHANGE_INFO',
    'BTC',
    'DOGE',
    'BTC_MARK',
    'DOGE_MARK',
    'BTC_TICKER',
    'MARGIN_ACCOUNT',
    'POSITION',
    'OPEN_ORDER',
    'USER_TRADE',
    'KLINE',
  ]);
  const declarations = [];
  for (const statement of tree.statements) {
    if (!ts.isVariableStatement(statement)) continue;
    for (const declaration of statement.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name) && wanted.has(declaration.name.text))
        declarations.push(`const ${declaration.getText(tree)};`);
    }
  }
  if (declarations.length !== wanted.size) throw new Error('Retained Options fixture census changed');
  const fixture = runInNewContext(`${declarations.join('\n')}\n({${[...wanted].join(',')}})`, {}, { timeout: 1000 });
  const origin = 'https://eapi.binance.com';
  const records = [{ t: 0, type: 'meta', private: !isPublic, epoch: 1792454400000, source: 'retained-ts-reference' }];
  const http = (method, path, params, payload, extra = {}) =>
    records.push({
      t: 0,
      type: 'http',
      method,
      url: `${origin}/eapi/v1/${path}${params ? `?${new URLSearchParams(params)}` : ''}`,
      status: 200,
      body: JSON.stringify(payload),
      ...extra,
    });
  const command = (t, op, args) => records.push({ t, type: 'command', op, ...args });
  http('GET', 'exchangeInfo', null, fixture.EXCHANGE_INFO);
  http('GET', 'ticker', null, [fixture.BTC_TICKER]);
  http('GET', 'mark', null, [fixture.BTC_MARK, fixture.DOGE_MARK]);
  if (!isPublic) {
    http('GET', 'marginAccount', null, fixture.MARGIN_ACCOUNT);
    http('GET', 'position', null, [fixture.POSITION]);
    http('GET', 'openOrders', null, [fixture.OPEN_ORDER]);
    command(400, 'placeOrders', {
      orders: [
        {
          symbol: fixture.BTC,
          type: 'limit',
          side: 'buy',
          amount: 0.13,
          price: 1004,
          timeInForce: 'PostOnly',
          clientOrderId: 'options-caller',
        },
      ],
    });
    http(
      'POST',
      'order',
      {
        symbol: fixture.BTC,
        side: 'BUY',
        type: 'LIMIT',
        quantity: '0.13',
        price: '1005',
        timeInForce: 'GTC',
        postOnly: 'true',
        clientOrderId: 'options-caller',
      },
      {
        ...fixture.OPEN_ORDER,
        orderId: '4611875134427365001',
        clientOrderId: 'options-caller',
        quantity: '0.13',
        executedQty: '0',
        price: '1005',
        status: 'ACCEPTED',
      },
    );
    command(800, 'cancelOrders', { ids: [fixture.OPEN_ORDER.orderId] });
    http(
      'DELETE',
      'order',
      { symbol: fixture.BTC, orderId: fixture.OPEN_ORDER.orderId },
      { ...fixture.OPEN_ORDER, status: 'CANCELED' },
    );
    command(1200, 'fetchFills', { symbol: fixture.BTC, limit: 100 });
    http('GET', 'userTrades', { symbol: fixture.BTC, limit: '100' }, [fixture.USER_TRADE]);
    command(1600, 'fetchOrderHistory', { symbol: fixture.BTC, limit: 100 });
    http('GET', 'historyOrders', { symbol: fixture.BTC, limit: '100' }, [fixture.OPEN_ORDER]);
  }
  records.sort((a, b) => a.t - b.t);
  return records;
}

if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) {
  const { values } = parseArgs({ options: { out: { type: 'string' }, public: { type: 'boolean', default: false } } });
  const records = buildBinanceOptionsTape({ isPublic: values.public });
  const out = values.out ?? `sdk-rs/harness/tapes/binanceoptions${values.public ? '' : '-private'}.jsonl`;
  mkdirSync(dirname(out), { recursive: true });
  writeFileSync(out, records.map((record) => JSON.stringify(record)).join('\n') + '\n');
  console.log(
    JSON.stringify({ out, records: records.length, source: 'packages/safe-cex/src/__tests__/binanceoptions.test.ts' }),
  );
}
