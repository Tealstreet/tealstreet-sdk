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
