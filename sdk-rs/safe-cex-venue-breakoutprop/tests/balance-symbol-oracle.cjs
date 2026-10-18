const fs = require('node:fs');
const { BreakoutpropExchange, store } = require('./portfolio_oracle.cjs');
const input = JSON.parse(fs.readFileSync(0, 'utf8'));
const exchange = new BreakoutpropExchange('breakoutprop', {}, store());
const balance = exchange.formatBalance(input);
const bits = value => {
  const buffer = Buffer.alloc(8);
  buffer.writeDoubleBE(value);
  return buffer.toString('hex');
};
const symbol = balance.info.portfolios[0].positions[0].symbol;
process.stdout.write(JSON.stringify({
  total: bits(balance.total), free: bits(balance.free),
  used: bits(balance.used), upnl: bits(balance.upnl),
  symbolUnits: Array.from({ length: symbol.length }, (_, i) => symbol.charCodeAt(i)),
  errors: exchange.errors,
}));
