// Child-only mutations: production TypeScript files are never edited.
const fs=require('node:fs');
const original=fs.readFileSync;
fs.readFileSync=function(file,...args){
 let value=original.call(this,file,...args);
 const name=String(file),mode=process.env.KUCOIN_ORACLE_MUTATION;
 const rules=mode==='store-own-proto' && name.endsWith('/store.base.ts')
   ? ['this.marketsBySymbolMap[market.symbol] = market;', 'Object.defineProperty(this.marketsBySymbolMap, market.symbol, {value:market, enumerable:true, configurable:true, writable:true});']
   : mode==='mapper-prototype' && name.endsWith('/kucoin.types.ts')
   ? ['export const ORDER_STATUS: Record<string, OrderStatus> = {', 'export const ORDER_STATUS: Record<string, OrderStatus> = { __proto__: null,']
   : mode==='balance-prototype' && name.endsWith('/kucoinspot.exchange.ts')
   ? ["const currencies: NonNullable<Balance['currencies']> = {};", "const currencies: NonNullable<Balance['currencies']> = Object.create(null);"]
   : mode==='table' && name.endsWith('/kucoin.types.ts')
   ? ['export const RECV_WINDOW = 20_000;', 'export const RECV_WINDOW = 20_001;']
   : mode==='retry' && name.endsWith('.api.ts')
   ? ['retries: 3,', 'retries: 0,']
   : mode==='factory-public' && name.endsWith('/kucoinspot.api.ts')
   ? ['if (isPublicEndpoint(config.url) || options.publicDataOnly)', 'if (options.publicDataOnly)']
   : mode==='missing-nanoid' && name.endsWith('/api-retry-capture.cjs')
   ? ["runtimeRequire('nanoid')", "runtimeRequire('kucoin-intentionally-missing-nanoid')"]
   : mode==='missing-retry' && name.endsWith('/api-retry-capture.cjs')
   ? ["runtimeRequire('axios-retry')", "runtimeRequire('kucoin-intentionally-missing-retry')"]
   : mode==='missing-axios' && name.endsWith('/api-capture.cjs')
   ? ["runtimeRequire('axios')", "runtimeRequire('kucoin-intentionally-missing-axios')"] : null;
 if(rules){const text=value.toString();if(!text.includes(rules[0]))throw Error('mutation target missing');const changed=mode==='store-own-proto'?text.replaceAll(rules[0],rules[1]):text.replace(rules[0],rules[1]);value=typeof value==='string'?changed:Buffer.from(changed);}
 return value;
};
