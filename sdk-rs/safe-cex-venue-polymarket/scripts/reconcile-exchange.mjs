import {readFileSync,writeFileSync} from 'node:fs';
import {parseManifest,serializeManifest} from '../../parity/extract-ts/src/manifest.ts';
const path='sdk-rs/parity/manifest/exchanges/polymarket/polymarket.exchange.ts.toml';
const manifest=parseManifest(readFileSync(path,'utf8'),'packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts');
const file='safe-cex-venue-polymarket/src/exchange.rs',prefix=file+'::';
const source=readFileSync('sdk-rs/'+file,'utf8');
const host=source.slice(source.indexOf('pub trait ExchangeHost'),source.indexOf('/// Detached promises'));
const structural=['ExchangeHost',...Array.from(host.matchAll(/\b(?:fn|type) (\w+)/g),m=>'ExchangeHost::'+m[1]),'PinTask','PolymarketExchange','PolymarketExchangeClass','PolymarketExchangeClass::new','PolymarketExchange::receiver'];
const rows=new Map([['PolymarketExchange',structural],['PolymarketExchange.constructor',['PolymarketExchange::new']],...['fetchMarkets','fetchTickers','placeOrder','updateOrder','fetchOHLCV','listenOHLCV'].map((name,i)=>{const rust=['fetch_markets','fetch_tickers','place_order','update_order','fetch_ohlcv','listen_ohlcv'][i];return ['PolymarketExchange.'+name,[rust,'PolymarketExchange::'+rust]];})]);
let moved=0;
for(const row of manifest.symbol){const names=rows.get(row.ts);if(!names)continue;if(row.status==='pending')moved++;else if(row.status!=='ported')throw Error('unexpected prior '+row.ts);row.rs=names.map(name=>prefix+name);row.status='ported';row.layer='mirror';row.note='Rename: '+row.rs.join(', ')+'. tests/exchange.rs executes actual source and native bodies, with native assertions before projections. Class/BaseExchange, existing method bodies, child constructors and language intrinsics are explicit executable dependencies; class witnesses cover declaration slots, maps, callbacks and composed market/account calls across two instances. Factory/SDK/transport internals are separate rows. Fixture floor: 16 per family, stopped at listen/15. See EXCHANGE_EVIDENCE.md for SHA-bound measured gates and controls.';}
if(!manifest.file.rust_files.includes(file))manifest.file.rust_files.push(file);
writeFileSync(path,serializeManifest(manifest));console.log(JSON.stringify({rows_pending_to_ported:moved,declared_rows:rows.size}));
