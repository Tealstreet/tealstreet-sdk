const leaf = () => ({ used: 0, free: 0, total: 0, upnl: 0 });
const seeds = ['USDT', 'USDC', 'BUSD', '10', '2', '0', '01', '1.0', '-1', '4294967294', '4294967295', '4294967296', '😀', '\ufeff'];
const rows = [];
for (let i = 0; i < seeds.length; i++) for (let j = 0; j < seeds.length; j++) {
  const keys = [seeds[i], seeds[j], 'USDT', 'USDC', '2', '10'];
  const currencies = {};
  const coins = {};
  for (const key of keys) {
    currencies[key] = leaf();
    coins[key] = { base: key, amount: 0, notional: 0 };
  }
  const nested = {}; for (const key of [...keys].reverse()) nested[key] = leaf();
  currencies.USDT.currencies = nested;
  const balance = { ...leaf(), currencies, coins };
  rows.push({ keys, balance, outer: Object.keys(currencies), nested: Object.keys(nested), coins: Object.keys(coins) });
}
process.stdout.write(JSON.stringify(rows));
