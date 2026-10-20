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

/** Phemex's execution, cumulative-size and flat-settings oracle uses venue rows only. */
export function fixPhemexAccount(account, tapeRecords) {
    // TS deduplicates executions by order within each message, then repeats
    // executions echoed by later messages. Build the oracle only from the tape.
    const seen = new Set();
    account.fills = [];
    for (const record of tapeRecords) {
      if (record.type !== 'ws') continue;
      const message = JSON.parse(record.text);
      if (message.type === 'snapshot') continue;
      for (const row of message.orders_p ?? []) {
        if (!['MakerFill', 'TakerFill'].includes(row.execStatus) || !row.execID || seen.has(row.execID)) continue;
        seen.add(row.execID);
        account.fills.push({ id: row.orderID, symbol: row.symbol.toUpperCase(), side: row.side.toLowerCase(), amount: Number(row.execQty), price: Number(row.execPriceRp) });
      }
    }
    const cumulative = new Map();
    for (const record of tapeRecords) {
      if (record.type !== 'ws') continue;
      for (const row of JSON.parse(record.text).orders_p ?? []) {
        const filled = Number(row.cumQty ?? row.cumQtyRq);
        const amount = Number(row.orderQty ?? row.orderQtyRq);
        if (Number.isFinite(filled) && Number.isFinite(amount)) cumulative.set(row.orderID, { filled, remaining: Math.max(0, amount - filled) });
      }
    }
    for (const order of account.orders) {
      const fixed = cumulative.get(order.id);
      if (fixed) Object.assign(order, fixed);
    }
    // TS uses last execution size, then zeroes canceled fills; cumulative venue
    // quantities above retain exposure independently of Rust's mapped rows.
    // Other native safety regressions: failed slices retain prior currency;
    // final events fence stale snapshots, Triggered remains non-terminal;
    // ambiguous writes never retry; split units preserve exact requested size;
    // partial-fill cancel/replace refuses uncertain remaining quantity;
    // zero-open tickers report zero percentage, and a full WS snapshot replaces
    // old depth. TS public dispatch now preserves outer sequence for buffering,
    // explicitly requests depth0 (default30 leaves deep levels stale), and
    // replaces periodic full snapshots.

  const settings = new Map(account.settings.map((row) => [row.symbol, row]));
  const observed = new Map();
  const observe = (row, fallback) => {
    const leverage = Math.abs(Number(row.leverageRr));
    if (!leverage || !Number.isFinite(leverage) || !row.symbol) return;
    const symbol = row.symbol.toUpperCase(), side = row.posSide ?? 'Merged';
    const raw = row.transactTimeNs ?? fallback ?? 0;
    const version = /^\d+$/.test(String(raw)) ? BigInt(raw) : 0n;
    const key = symbol + ':' + side;
    if ((observed.get(key)?.version ?? -1n) > version) return;
    observed.set(key, { symbol, side, version, leverage, marginType: Number(row.leverageRr) < 0 || row.crossMargin ? 'cross' : 'isolated' });
  };
  // TS drops zero-sized rows. Use the latest authoritative row per side,
  // preferring long/merged leverage as the single market setting does.
  for (const record of [...tapeRecords].sort((a,b) => (a.effectiveAt ?? a.availableAfter ?? a.t ?? 0) - (b.effectiveAt ?? b.availableAfter ?? b.t ?? 0))) {
    if (record.type === 'http' && record.status === 200 && new URL(record.url).pathname === '/g-accounts/accountPositions') {
      for (const row of parse(record.body).data?.positions ?? []) observe(row);
    } else if (record.type === 'ws') {
      const message = parse(record.text);
      for (const row of message.positions_p ?? []) observe(row, message.timestamp);
    }
  }
  const missing = new Map();
  for (const row of observed.values()) {
    if (settings.has(row.symbol)) continue;
    const previous = missing.get(row.symbol);
    if (!previous || previous.side === 'Short' || row.side !== 'Short') missing.set(row.symbol, row);
  }
  for (const {symbol,leverage,marginType} of missing.values()) settings.set(symbol,{symbol,leverage,marginType});
  account.settings = [...settings.values()].sort((a,b) => a.symbol.localeCompare(b.symbol));
}
