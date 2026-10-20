// Expected conditional values come from the recorded venue, never the Rust output.
const parse = (text) => JSON.parse(text, (_key, value, context) => {
  if (typeof value === 'number' && !Number.isSafeInteger(value) && Number.isInteger(value)) {
    if (!context?.source) throw new Error('lossless JSON number source unavailable');
    if (/^-?\d+$/.test(context.source)) return context.source;
  }
  return value;
});
export function fixWeexConditionals(account, records) {
  const expected = new Map();
  const observe = (row) => {
    const id = row.algoId ?? row.orderId ?? row.id;
    if (id == null) return;
    const type = row.orderType ?? row.type;
    const trigger = Number(row.triggerPrice);
    const protective = [true, 'true', '1', 1].includes(row.reduceOnly)
      || [true, 'true', '1', 1].includes(row.closePosition)
      || [true, 'true', '1', 1].includes(row.positionTpsl)
      || ['TPSL', 'TP_SL'].includes(row.algoType)
      || ['TAKE_PROFIT', 'STOP_LOSS'].includes(row.planType);
    if (!protective && ['STOP', 'STOP_MARKET', 'TAKE_PROFIT', 'TAKE_PROFIT_MARKET'].includes(type)
      && Number.isFinite(trigger) && trigger > 0) expected.set(String(id), trigger);
    else expected.delete(String(id));
  };
  for (const record of [...records].sort((a, b) => (a.effectiveAt ?? a.t) - (b.effectiveAt ?? b.t))) {
    if (record.type === 'http' && record.status >= 200 && record.status < 300) {
      if (record.method === 'GET' && record.url.includes('/openAlgoOrders')) {
        const rows = parse(record.body);
        if (Array.isArray(rows)) rows.forEach(observe);
      } else if (record.method === 'POST' && new URL(record.url).pathname.endsWith('/algoOrder') && record.requestBody) {
        observe({ ...parse(record.requestBody), ...parse(record.body) });
      }
    } else if (record.type === 'ws') {
      const message = parse(record.text);
      if (message.e === 'orders' && Array.isArray(message.d)) message.d.forEach(observe);
    }
  }
  for (const order of account.orders) {
    if (!order.reduceOnly && expected.has(order.id)
      && ['conditional', 'stop_market', 'take_profit_market'].includes(order.type)) {
      order.type = 'conditional';
      order.price = expected.get(order.id);
    }
  }
}

// BlofinMetadata.supportsPositionMargin is false, and its TS mapPositions omits
// marginSize. Store merges can retain a zero-size placeholder's marginSize=0;
// the unavailable field is null in the comparison, independently of Rust.
export function fixBlofinAccount(account) {
  for (const position of account.positions) {
    if (position.marginSize === 0) position.marginSize = null;
  }
  if (account.positions.length === 0) {
    account.balance.used = 0;
    account.balance.upnl = 0;
  }
}

// Bybit TS omits this native post-only rejection. Derive the correction from
// the latest recorded order event so an incorrect engine result stays visible.
export function fixBybitRejectReasons(account, records) {
  const latest = new Map();
  for (const record of records) {
    if (record.type !== 'ws') continue;
    const event = parse(record.text);
    if (event.topic === 'order') for (const row of event.data) latest.set(String(row.orderId), row);
  }
  for (const order of account.orders) {
    if (order.rejectReason === null && order.status === 'canceled'
      && latest.get(order.id)?.rejectReason === 'EC_PostOnlyWillTakeLiquidity') {
      order.rejectReason = 'PostOnly';
    }
  }
}

// TS emits the FILLED order total and the execution for one Lighter trade.
// Build its corrected fill oracle from recorded execution IDs and quantities.
export function fixLighterFills(account, records) {
  const markets = new Map();
  const marketIds = new Map();
  for (const record of records) {
    if (record.type !== 'http' || !/\/orderBook(?:Details|s)$/.test(new URL(record.url).pathname) || record.status !== 200) continue;
    const payload = parse(record.body);
    for (const market of payload.order_book_details ?? payload.order_books ?? []) {
      markets.set(String(market.market_id), market.symbol);
      marketIds.set(market.symbol, String(market.market_id));
    }
  }
  const flatten = (value, market) => Array.isArray(value) ? value.flatMap((row) => flatten(row, market)) : value && typeof value === 'object'
    ? ('price' in value && ('size' in value || 'amount' in value)) ? [{fill:value,market}]
      : Object.entries(value).flatMap(([key,row]) => flatten(row,key)) : [];
  const fills = new Map();
  const seen = new Set();
  let observed = false;
  const accountId = records.find((record) => record.type === 'config')?.options?.applicationId;
  for (const record of records) {
    if (record.type !== 'ws') continue;
    const message = parse(record.text);
    const kind = String(message.type ?? '');
    if (!String(message.channel ?? '').startsWith('account_all') || !/^(update|subscribed)/.test(kind)) continue;
    const payload = message.data ?? message;
    if (payload.fills === undefined && payload.trades === undefined) continue;
    observed = true;
    for (const {fill,market:hint} of [payload.fills,payload.trades].flatMap((source) => flatten(source))) {
      const execution = fill.trade_id_str ?? fill.tradeId ?? fill.trade_id ?? fill.fillId ?? fill.fill_id;
      const market = String(fill.market_id ?? fill.marketId ?? hint ?? '');
      const symbol = fill.symbol || markets.get(market);
      const side = String(fill.ask_account_id ?? '') === String(accountId) ? 'sell'
        : String(fill.bid_account_id ?? '') === String(accountId) ? 'buy' : String(fill.side ?? '').toLowerCase();
      const id = fill.orderId ?? fill.order_id ?? (side === 'sell' ? fill.ask_id : fill.bid_id);
      const amount = Number(fill.size ?? fill.amount), price = Number(fill.price);
      if (execution == null || String(execution) === '' || !symbol || !['buy','sell'].includes(side) || id == null || !(amount > 0) || !(price > 0)) continue;
      const key = `${marketIds.get(symbol) ?? (market || symbol)}:${execution}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (kind.startsWith('subscribed')) continue;
      fills.set(key, {id:String(id),symbol,side,amount,price});
    }
  }
  if (observed) account.fills = [...fills.values()];
}

export function fixLighterCommands(results, records) {
  const commands=records.filter(record=>record.type==='command');
  for(let index=0;index<commands.length;index++) {
    const command=commands[index];
    // The venue is net only. A request to retain that mode needs no write;
    // TS inherits the base unsupported method instead of acknowledging it.
    if(command.op==='setPositionMode' && command.hedged===false && results[index]?.error==='Not implemented') results[index]={ok:null};
  }
}
