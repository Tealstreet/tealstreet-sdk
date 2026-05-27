#!/usr/bin/env tsx

import { MDG } from '@tealstreet/grvt-client';

async function main() {
  const mdg = new MDG({ host: 'https://market-data.grvt.io' });
  const instruments = await mdg.allInstruments({});
  const btc = instruments.result?.find((i: any) => i.instrument === 'BTC_USDT_Perp');
  console.log(JSON.stringify(btc, null, 2));
}

main().catch(console.error);
