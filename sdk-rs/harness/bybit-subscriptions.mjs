// Bybit permits all-in-one and category-specific private topics:
// https://bybit-exchange.github.io/docs/v5/websocket/private/order
// Normalize only the TS oracle; Rust's category, URL and generation stay exact.
export const normalizeTsBybitSubscriptions = (profile, subscriptions, privateUrl) => {
  const category = profile === 'bybitinverse' ? 'inverse' : profile === 'bybitspot' ? 'spot' : undefined;
  if (!category || !privateUrl || !Array.isArray(subscriptions)) return subscriptions;
  return subscriptions
    .map((key) => {
      const wire = /^([1-9]\d*) WS (\S+) (\{.*\})$/.exec(key);
      if (wire) {
        if (wire[2] !== `${privateUrl}?`) return key;
        let message;
        try {
          message = JSON.parse(wire[3]);
        } catch {
          return key;
        }
        if (
          JSON.stringify(Object.keys(message).sort()) !== '["args","op"]' ||
          !['subscribe', 'unsubscribe'].includes(message.op) ||
          !Array.isArray(message.args) ||
          message.args.length !== 1 ||
          !['execution', 'order', 'position'].includes(message.args[0]) ||
          (category === 'spot' && message.args[0] === 'position') ||
          JSON.stringify({ args: message.args, op: message.op }) !== wire[3]
        )
          return key;
        return `${wire[1]} WS ${wire[2]} ${JSON.stringify({ args: [`${message.args[0]}.${category}`], op: message.op })}`;
      }
      const match = /^([1-9]\d*) (\S+) (subscribe|unsubscribe) (execution|order|position)$/.exec(key);
      if (!match || match[2] !== privateUrl || (category === 'spot' && match[4] === 'position')) return key;
      return `${match[1]} ${match[2]} ${match[3]} ${match[4]}.${category}`;
    })
    .sort();
};
