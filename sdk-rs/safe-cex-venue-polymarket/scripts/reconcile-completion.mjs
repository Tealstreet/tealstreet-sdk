import {readFileSync,writeFileSync} from 'node:fs';
import {parseManifest,serializeManifest} from '../../parity/extract-ts/src/manifest.ts';
const base='sdk-rs/parity/manifest/exchanges/polymarket/';
const classes='safe-cex-venue-polymarket/src/classes.rs',factory='safe-cex-venue-polymarket/src/api_factory.rs';
const target=(file,names)=>names.map(name=>file+'::'+name);
const traitNames=(file,name)=>{const s=readFileSync('sdk-rs/'+file,'utf8');const part=s.slice(s.indexOf('pub trait '+name),s.indexOf('\n}',s.indexOf('pub trait '+name)));return [name,...Array.from(part.matchAll(/\b(?:fn|type) (\w+)/g),m=>name+'::'+m[1])];};
const maps={
 'api':new Map([
 ['createPolymarketAPI',target(factory,['create_polymarket_api',...traitNames(factory,'FactoryHost')])],
 ['PolymarketAPI',target(classes,['PolymarketAPI','PolymarketAPI::new','PolymarketAPI::receiver','PolymarketAPI::call','PolymarketClasses','PolymarketClasses::new',...traitNames(classes,'ClassHost')])],
 ['PolymarketAPI#getCredentials',target(classes,['PolymarketAPI::new','CredentialCallback','CredentialCallback::value','CredentialCallback::call'])],
 ]),
 'ws-public':new Map([
 ['PolymarketPublicWebSocket',target(classes,['PolymarketPublicWebSocket','PolymarketPublicWebSocket::new','PolymarketPublicWebSocket::receiver','PolymarketPublicWebSocket::dispatch'])],
 ['PolymarketPublicWebSocket#handlePriceChange',target(classes,['handle_price_change','PolymarketPublicWebSocket::new'])],
 ['PolymarketPublicWebSocket#handleOrderBookUpdate',target(classes,['handle_order_book_update'])],
 ]),
 'ws-private':new Map([
 ['PolymarketPrivateWebSocket',target(classes,['PolymarketPrivateWebSocket','PolymarketPrivateWebSocket::new','PolymarketPrivateWebSocket::receiver'])],
 ['PolymarketPrivateWebSocket#getCredentials',target(classes,['PolymarketPrivateWebSocket::new','CredentialCallback','CredentialCallback::value','CredentialCallback::call'])],
 ]),
};
let moved=0;
for(const [file,rows] of Object.entries(maps)){
 const source='packages/safe-cex/src/exchanges/polymarket/polymarket.'+file+'.ts',path=base+'polymarket.'+file+'.ts.toml';
 const manifest=parseManifest(readFileSync(path,'utf8'),source);
 for(const row of manifest.symbol){const rs=rows.get(row.ts);if(!rs)continue;if(row.status==='pending')moved++;else if(row.status!=='ported')throw Error('unexpected '+row.ts);row.rs=rs;row.layer='mirror';row.status='ported';
 const details={
 'createPolymarketAPI':'Installs native request and both response callbacks, original config/client aliases, per-client rate state, posted-read retry arguments, structured/raw/undefined signing views and enhanced error prototype/descriptor. qs, retry and HTTP library internals are bindings.',
 'PolymarketAPI':'Evaluated class handle with 29 prototype method bindings and five declaration slots; reuses the existing constructor body. Native witnesses call the existing spread-read body through the captured prototype binding and compare shared prototype identity across two instances.',
 'PolymarketAPI#getCredentials':'The public get_credentials field stores a live callable property handle constructed by new. Indexed targets name its construction and execution: value observes the original function capability; call captures its current member receiver. Native witnesses assert function/result aliases, null and late credentials.',
 'PolymarketPublicWebSocket':'Evaluated BaseWebSocket subclass with independent Set/Map state, source field order, five late-bound handler topics and own arrow bindings. Native witnesses dispatch the price/book callbacks and compare prototype identity across two instances.',
 'PolymarketPublicWebSocket#handlePriceChange':'Actual inner body plus class registration through throttle(500,{leading:true,trailing:true}). Native fixtures assert distinct bid/ask, finite-field omission, unknown-token filtering and mark/index midpoint. Throttle internals are a bound dependency.',
 'PolymarketPublicWebSocket#handleOrderBookUpdate':'Source callback lookup, independent bid/ask accumulators, sparse map semantics and one book capability shared with live Set iteration. Native assertions precede comparison to the actual source handler.',
 'PolymarketPrivateWebSocket':'Evaluated BaseWebSocket subclass; declaration/arrow order, four late-bound handler topics, original parent and credential function slot, and shared prototype identity across two instances.',
 'PolymarketPrivateWebSocket#getCredentials':'The public get_credentials field stores a live callable property handle constructed by new. Indexed targets name its construction and execution: value observes the original function capability; call captures its current member receiver. Native witnesses assert function/result aliases, null and late credentials.',
 };
 row.note='Rename: '+rs.join(', ')+'. '+details[row.ts]+' tests/exchange.rs::completion executes actual source and native bodies: eight fixtures per family, stopped at book/7. Existing method bodies and base/runtime operations are executable bindings. See COMPLETION_EVIDENCE.md for SHA-bound measured gates and control floors.';
 }
 for(const rust of [classes,...(file==='api'?[factory]:[])])if(!manifest.file.rust_files.includes(rust))manifest.file.rust_files.push(rust);
 writeFileSync(path,serializeManifest(manifest));
}
console.log(JSON.stringify({rows_pending_to_ported:moved,declared_rows:8}));
