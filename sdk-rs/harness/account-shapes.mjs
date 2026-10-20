// Match the transfer model: financial fields and identity, without raw wire
// metadata or local observation timestamps. Both implementations use this path.
export const currencyBalances = (currencies = {}) => Object.fromEntries(
  Object.entries(currencies).map(([currency, { free, used, total, upnl }]) => [currency, { free, used, total, upnl }]),
);

export const lookupResult = (result, command) => {
  if (command?.op !== 'findOrderByClientOrderId' || result.ok?.found !== true) return result;
  const order = result.ok.order;
  const aliases = ['clOrdId', 'clientOrderId', 'orderLinkId', 'clientOid', 'clOrdID', 'c', 'client_id', 'client_order_id', 'clientId'];
  const nativeId = order.clientOrderId ?? aliases.map((field) => order.info?.[field]).find((id) => id !== undefined && id !== null);
  const identity = nativeId === undefined ? undefined : String(nativeId);
  const fields = ['id', 'parentId', 'symbol', 'status', 'type', 'side', 'price', 'amount', 'filled', 'remaining',
    'timeInForce', 'triggerType', 'tpTriggerType', 'slTriggerType', 'takeProfit', 'stopLoss', 'extra'];
  return { ...result, ok: { ...result.ok, order: {
    ...Object.fromEntries(fields.filter((field) => order[field] !== undefined).map((field) => [field, order[field]])),
    ...(identity === undefined ? {} : { clientOrderId: identity }),
    reduceOnly: Boolean(order.reduceOnly), close: Boolean(order.close),
  } } };
};
