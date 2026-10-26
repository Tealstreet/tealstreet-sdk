// Independent oracle for the documented Binance ACCOUNT_UPDATE and Bybit
// position payloads. Only replace TS's zero placeholder when tape proves IM.
export function fixPositionMargins(output, records, family) {
  const margins = new Map();
  const leverage = new Map();
  const key = (symbol, side) => `${symbol}:${side}`;
  const sideOf = (side, amount) => side === 'SHORT' || side === 'Sell' || Number(amount) < 0 ? 'short' : 'long';
  for (const record of records) {
    if (record.type === 'http' && record.status === 200) {
      const path = new URL(record.url).pathname;
      if (family === 'binance' && /\/symbolConfig$/.test(path)) {
        const body = JSON.parse(record.body);
        for (const row of body) leverage.set(row.symbol, Number(row.leverage));
      }
      if (family === 'bybit' && path === '/v5/position/list') {
        const body = JSON.parse(record.body);
        for (const row of body.result?.list ?? []) {
          margins.set(key(row.symbol, sideOf(row.side, row.size)), Number(row.positionIM) || null);
        }
      }
      continue;
    }
    if (record.type !== 'ws' || typeof record.text !== 'string') continue;
    let event;
    try { event = JSON.parse(record.text); } catch { continue; }
    if (family === 'binance' && event.e === 'ACCOUNT_UPDATE') {
      for (const row of event.a?.P ?? []) {
        const isolated = Number(row.iw);
        const lev = leverage.get(row.s);
        const notional = Math.abs(Number(row.pa) * Number(row.ep) + Number(row.up));
        if (isolated > 0 || lev > 0)
          margins.set(key(row.s, sideOf(row.ps, row.pa)), isolated > 0 ? isolated : notional / lev);
      }
    }
    if (family === 'bybit' && event.topic === 'position') {
      for (const row of event.data ?? []) {
        margins.set(key(row.symbol, sideOf(row.side, row.size)), Number(row.positionIM) || null);
      }
    }
  }
  for (const position of output.account?.positions ?? []) {
    const expected = margins.get(key(position.symbol, position.side));
    if (position.marginSize === 0 && Number.isFinite(expected) && expected > 0)
      position.marginSize = expected;
  }
}
