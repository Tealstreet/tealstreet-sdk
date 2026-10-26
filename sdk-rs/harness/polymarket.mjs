// Independent raw CLOB V2 protocol audit. No venue result is used as its oracle.
import { createHmac } from 'node:crypto';
import { createRequire } from 'node:module';
import { requestKey,subscriptionKeys } from './requests.mjs';
const require = createRequire(new URL('../../packages/safe-cex/package.json', import.meta.url));
let ethers, address;
export const POLYMARKET_REPLAY = {
  // Public deterministic Ethereum signing vector, never a funded account.
  key: `0x${'01'.repeat(32)}`,
  polymarket: { apiKey: 'poly-replay-key', apiSecret: Buffer.from('poly-replay-secret').toString('base64url'), apiPassphrase: 'poly-replay-pass', signerType: 0 },
};
function initializeCrypto() {
  if (ethers) return;
  ({ ethers } = require('ethers'));
  address = new ethers.Wallet(POLYMARKET_REPLAY.key).address;
}
const fields = [
  ['salt','uint256'],['maker','address'],['signer','address'],['tokenId','uint256'],
  ['makerAmount','uint256'],['takerAmount','uint256'],['side','uint8'],['signatureType','uint8'],
  ['timestamp','uint256'],['metadata','bytes32'],['builder','bytes32'],
].map(([name,type])=>({name,type}));
const zero32=`0x${'00'.repeat(32)}`;
export function verifyPolymarketRequest(request) {
  try { return verify(request); } catch { return false; }
}
function verify({method,url,headers={},body=''}) {
  const parsed=new URL(url);
  if (parsed.hostname!=='clob.polymarket.com') return ['GET'].includes(method);
  if (['/midpoints','/books'].includes(parsed.pathname)) return method==='POST';
  if (['/prices-history','/book','/time'].includes(parsed.pathname)) return method==='GET';
  // Standalone protocol tests may verify a request without replay setup.
  initializeCrypto();
  const h=Object.fromEntries(Object.entries(headers).map(([k,v])=>[k.toUpperCase(),String(v)]));
  if (h.POLY_ADDRESS!==address || !/^\d+$/.test(h.POLY_TIMESTAMP??'')) return false;
  const seconds=Number(h.POLY_TIMESTAMP);
  if (!Number.isSafeInteger(seconds)||String(seconds)!==h.POLY_TIMESTAMP||Math.abs(Date.now()/1000-seconds)>60) return false;
  if (parsed.pathname==='/auth/derive-api-key') {
    if (method!=='GET'||h.POLY_NONCE!=='0') return false;
    return ethers.verifyTypedData({name:'ClobAuthDomain',version:'1',chainId:137},{ClobAuth:[{name:'address',type:'address'},{name:'timestamp',type:'string'},{name:'nonce',type:'uint256'},{name:'message',type:'string'}]}, {address,timestamp:h.POLY_TIMESTAMP,nonce:'0',message:'This message attests that I control the given wallet'},h.POLY_SIGNATURE)===address;
  }
  if (h.POLY_API_KEY!==POLYMARKET_REPLAY.polymarket.apiKey||h.POLY_PASSPHRASE!==POLYMARKET_REPLAY.polymarket.apiPassphrase) return false;
  const expected=createHmac('sha256',Buffer.from(POLYMARKET_REPLAY.polymarket.apiSecret,'base64url')).update(h.POLY_TIMESTAMP+method+parsed.pathname+body).digest('base64').replace(/\+/g,'-').replace(/\//g,'_');
  if (h.POLY_SIGNATURE!==expected || Math.abs(Date.now()/1000-Number(h.POLY_TIMESTAMP))>60) return false;
  if (method==='POST'&&parsed.pathname==='/order') {
    const payload=JSON.parse(body),o=payload.order;
    if (payload.owner!==h.POLY_API_KEY||payload.orderType!=='GTC'||payload.postOnly!==false||payload.deferExec!==false||o.expiration!=='0') return false;
    if (Object.keys(o).sort().join(',')!==[...fields.map(f=>f.name),'expiration','signature'].sort().join(',')) return false;
    if (!Number.isSafeInteger(o.salt)||o.salt<0||!['BUY','SELL'].includes(o.side)||o.signatureType!==0) return false;
    const milliseconds=Number(o.timestamp);
    if (typeof o.timestamp!=='string'||String(milliseconds)!==o.timestamp||!Number.isSafeInteger(milliseconds)||Math.abs(Date.now()-milliseconds)>60_000) return false;
    if (o.maker!==address||o.signer!==address||o.metadata!==zero32||o.builder!==zero32) return false;
    const message={...o,side:o.side==='BUY'?0:1};delete message.signature;delete message.expiration;
    // Synthetic tape has standard tokens1/2 and negative-risk tokens3/4.
    const domain={name:'Polymarket CTF Exchange',version:'2',chainId:137,verifyingContract:['3','4'].includes(o.tokenId)?'0xe2222d279d744050d28e00520010520000310F59':'0xE111180000d2663C0091e4f400237545B87B996B'};
    if (ethers.verifyTypedData(domain,{Order:fields},message,o.signature)!==address) return false;
  }
  return true;
}
export const polymarket = {
  rust:'polymarket',
  virtualPrivateClock: true,
  // Only the selected venue loads its independent crypto oracle; do it before
  // replay takes the memory baseline, symmetrically for TS and Rust.
  initializeReplay: initializeCrypto,
  // Discovery may enumerate many pages; capture the full requested live window.
  recordAfterReady:true,
  publicOptions:{polymarket:{marketSlugs:['xi-jinping-out-before-2027','will-gavin-newsom-win-the-2028-democratic-presidential-nomination-568']}},
  ts:{module:'../../packages/safe-cex/src/__tests__/reference/polymarket/polymarket.exchange.ts',className:'PolymarketExchange',name:'polymarket'},
  matchRequestBody:true,
  isWrite:(method,url)=>method!=='GET'&&!['/midpoints','/books'].includes(new URL(url).pathname),
  privateSocket:'/ws/user',
  compareFillPrices:true,
  // Unlevered outcome holdings have no account margin/leverage settings.
  tsSettings:()=>[],
  startupSubscriptions(records,books) {
    const tokens=new Map();
    for(const record of records) {
      if(record.type!=='http'||record.status<200||record.status>=300||new URL(record.url).hostname!=='gamma-api.polymarket.com') continue;
      const page=JSON.parse(record.body);
      for(const market of page.markets??[]) {
        const ids=JSON.parse(market.clobTokenIds??'[]'),outcomes=JSON.parse(market.outcomes??'[]');
        if(ids.length!==outcomes.length) throw new Error('Invalid taped Gamma outcomes');
        ids.forEach((id,i)=>tokens.set(`${market.slug}-${outcomes[i]}`,id));
      }
    }
    const expected=[];
    for(const symbol of books) {
      const id=tokens.get(symbol);if(!id) throw new Error(`Missing taped Polymarket outcome ${symbol}`);
      expected.push(...subscriptionKeys('wss://ws-subscriptions-clob.polymarket.com/ws/market',{type:'market',assets_ids:[id],custom_feature_enabled:true},new Set(),this));
    }
    const config=records.find(r=>r.type==='config');
    const metadata=records.find(r=>r.type==='meta');
    if(config?.private??metadata?.private) {
      const provided=config?.options?.polymarket;
      const derived=records.find(r=>r.type==='http'&&new URL(r.url).pathname==='/auth/derive-api-key'&&r.status===200);
      const credentials=provided?.apiKey?{apiKey:provided.apiKey,secret:provided.apiSecret,passphrase:provided.apiPassphrase}:derived&&JSON.parse(derived.body);
      if(!credentials?.apiKey||!credentials.secret||!credentials.passphrase) throw new Error('Missing taped private channel credentials');
      expected.push(...subscriptionKeys('wss://ws-subscriptions-clob.polymarket.com/ws/user',{type:'user',auth:credentials},new Set(),this));
    }
    return expected;
  },
  tsBooksSubscribed:(exchange,count)=>exchange.publicWebsocket.subscribedAssets.size>=count,
  verifyRequest:verifyPolymarketRequest,
  requestBody:(body,{url,method})=>method.toUpperCase()==='POST'&&new URL(url).pathname==='/order'?{...body,order:{...body.order,salt:'<random>',timestamp:'<time>',signature:'<verified>'}}:body,
  requestKey(method,url,body,callerIds){return requestKey(method,url,body,callerIds,this);},
  subscriptionMessage(message){
    if (message.type==='market'||message.operation) return {op:message.operation??'subscribe',args:message.assets_ids.map(id=>({asset_id:id,custom_feature_enabled:message.custom_feature_enabled===true}))};
    return message;
  },
};
