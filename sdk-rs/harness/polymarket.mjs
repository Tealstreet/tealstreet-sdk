// Independent raw CLOB V2 protocol audit. No venue result is used as its oracle.
import { uniqueRequestHeaders } from './protocol-headers.mjs';
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
const uint256 = (value) => typeof value === 'string' && value.length > 0 && value.length <= 78 && !/[^0-9]/.test(value) && (value === '0' || value[0] !== '0') && BigInt(value) < (1n << 256n);
const zero32=`0x${'00'.repeat(32)}`;
export function verifyPolymarketRequest(request) {
  try { return verify(request); } catch { return false; }
}
function verify({method,url,headers={},body=''}) {
  if (typeof url !== 'string' || /[\x00-\x20\x7f]/.test(url)) return false;
  const parsed=new URL(url), unique=uniqueRequestHeaders(headers);
  if (parsed.protocol!=='https:'||parsed.username||parsed.password||parsed.port||parsed.hash) return false;
  const credentialed=Object.keys(unique).some((name)=>name.startsWith('poly_')||['authorization','coinbase-authorization','cookie'].includes(name));
  if (parsed.hostname==='gamma-api.polymarket.com') return !credentialed&&method==='GET'&&parsed.pathname==='/markets/keyset';
  if (parsed.hostname==='data-api.polymarket.com') return !credentialed&&method==='GET'&&parsed.pathname==='/positions';
  if (parsed.hostname!=='clob.polymarket.com') return false;
  if (['/midpoints','/books','/prices','/spreads','/last-trades-prices'].includes(parsed.pathname)) return !credentialed&&method==='POST';
  if (['/prices-history','/book','/time','/markets','/simplified-markets','/price','/midpoint','/spread','/last-trade-price'].includes(parsed.pathname)) return !credentialed&&method==='GET';
  // Standalone protocol tests may verify a request without replay setup.
  initializeCrypto();
  const h=Object.fromEntries(Object.entries(unique).map(([k,v])=>[k.toUpperCase(),v]));
  if (h.POLY_ADDRESS!==address || !/^(?:0|[1-9][0-9]*)$/.test(h.POLY_TIMESTAMP??'')) return false;
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
    for (const key of ['tokenId','makerAmount','takerAmount']) if (!uint256(o[key])) return false;
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
// Socket identity, not URL/generation text, owns authentication and asset scope.
export function createPolymarketSocketAudit(_impl, { isPrivate, books = [], records = [] }) {
  const sockets = new Map(), errors = [], tokens = new Map(), conditions = new Map();
  for (const r of records) {
    if (r.type !== 'http' || r.status < 200 || r.status >= 300 || new URL(r.url).hostname !== 'gamma-api.polymarket.com') continue;
    for (const market of JSON.parse(r.body).markets ?? []) {
      const ids = JSON.parse(market.clobTokenIds ?? '[]'), outcomes = JSON.parse(market.outcomes ?? '[]');
      ids.forEach((id, i) => { tokens.set(`${market.slug}-${outcomes[i]}`, id); if (market.conditionId) conditions.set(id, market.conditionId); });
    }
  }
  const expected = books.map((symbol) => tokens.get(symbol));
  const config = records.find((r) => r.type === 'config');
  const supplied = config?.options?.polymarket;
  const derived = records.find((r) => r.type === 'http' && new URL(r.url).pathname === '/auth/derive-api-key' && r.status === 200);
  const credentials = supplied?.apiKey ? { apiKey: supplied.apiKey, secret: supplied.apiSecret, passphrase: supplied.apiPassphrase }
    : derived ? JSON.parse(derived.body) : { apiKey: POLYMARKET_REPLAY.polymarket.apiKey, secret: POLYMARKET_REPLAY.polymarket.apiSecret, passphrase: POLYMARKET_REPLAY.polymarket.apiPassphrase };
  const get = (url, socket = 1) => {
    const key = socket !== null && typeof socket === 'object' ? socket : `${url} ${socket}`;
    if (!sockets.has(key)) {
      const u = new URL(url), privateSocket = u.pathname === '/ws/user';
      const valid = u.protocol === 'wss:' && u.hostname === 'ws-subscriptions-clob.polymarket.com' && !u.port && !u.username && !u.password && !u.search && !u.hash
        && ['/ws/user', '/ws/market'].includes(u.pathname) && (socket?.channel === undefined || socket.channel === (privateSocket ? 'private' : 'public'));
      if (!valid) errors.push('Polymarket invalid socket origin or role');
      sockets.set(key, { valid, private: privateSocket, closed: false, authenticated: false, assets: new Set(), pings: 0 });
    }
    return sockets.get(key);
  };
  const active = (url, socket) => {
    const state = get(url, socket);
    if (state.closed) errors.push('Polymarket traffic on closed socket');
    return state.valid && !state.closed ? state : undefined;
  };
  return {
    connected(url, socket) { get(url, socket); },
    closed(url, socket) { get(url, socket).closed = true; },
    sent(url, text, socket) {
      const state = active(url, socket); if (!state) return;
      if (text === 'PING') { state.pings++; return; }
      let m; try { m = JSON.parse(text); } catch { errors.push('Polymarket sent invalid JSON'); return; }
      if (state.private) {
        if (m?.type !== 'user' || m.assets_ids !== undefined || !m.auth || Object.keys(m.auth).sort().join(',') !== 'apiKey,passphrase,secret'
          || Object.entries(credentials).some(([key, value]) => typeof value !== 'string' || !value || m.auth[key] !== value)) errors.push('Polymarket invalid private subscription credentials');
        else state.authenticated = true;
        return;
      }
      if (m?.auth !== undefined || !((m.type === 'market' && m.operation === undefined) || (m.type === undefined && ['subscribe', 'unsubscribe'].includes(m.operation)))
        || m.custom_feature_enabled !== true || !Array.isArray(m.assets_ids) || !m.assets_ids.length
        || m.assets_ids.some((id) => !uint256(id))) {
        errors.push('Polymarket invalid public asset subscription'); return;
      }
      for (const id of m.assets_ids) if (m.operation === 'unsubscribe') state.assets.delete(id); else state.assets.add(id);
    },
    received(url, text, socket) {
      const state = active(url, socket); if (!state) return;
      // The venue can send unsolicited health PONGs as well as replies.
      if (text === 'PONG') { state.pings = Math.max(0, state.pings - 1); return; }
      let rows; try { rows = JSON.parse(text); } catch { errors.push('Polymarket received invalid JSON'); return; }
      for (const row of Array.isArray(rows) ? rows : [rows]) {
        if (state.private) {
          if (!state.authenticated || !['order', 'trade'].includes(row?.event_type)) errors.push('Polymarket received unauthenticated private event');
        } else {
          if (!['book', 'price_change', 'best_bid_ask', 'last_trade_price', 'tick_size_change', 'new_market', 'market_resolved'].includes(row?.event_type)) { errors.push('Polymarket received unknown public event'); continue; }
          // Feature-enabled market sockets receive discovery broadcasts for all new markets.
          if (row.event_type === 'new_market') { if (!state.assets.size) errors.push('Polymarket discovery before subscription'); continue; }
          const assets = [row.asset_id, ...(row.price_changes ?? []).map((change) => change.asset_id), ...(row.assets_ids ?? [])].filter((id) => id !== undefined);
          // Price changes include both outcomes of a subscribed condition. The
          // recorded Gamma condition proves siblings; arbitrary extra assets fail.
          const related = row.event_type === 'price_change' && assets.some((id) => state.assets.has(id))
            && assets.every((id) => conditions.get(id) === row.market && typeof row.market === 'string');
          if (!assets.length || (!related && assets.some((id) => !state.assets.has(id)))) errors.push('Polymarket received unsubscribed asset');
        }
      }
    },
    ready() {
      const active = [...sockets.values()].filter((state) => state.valid && !state.closed);
      return expected.every((id) => id !== undefined && active.some((state) => !state.private && state.assets.has(id)))
        && (!isPrivate || active.some((state) => state.private && state.authenticated));
    },
    errors() { return [...errors, ...(this.ready() ? [] : ['Polymarket missing required socket subscriptions'])]; },
  };
}

export const polymarket = {
  rust:'polymarket',
  preserveBodyFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  preserveQueryFields: ['timestamp', 'recvWindow', 'signature', 'requestId'],
  generatedId: () => false,
  // Only the selected venue loads its independent crypto oracle; do it before
  // replay takes the memory baseline, symmetrically for TS and Rust.
  initializeReplay: initializeCrypto,
  // Discovery may enumerate many pages; capture the full requested live window.
  recordAfterReady:true,
  publicOptions:{polymarket:{marketSlugs:['xi-jinping-out-before-2027','will-gavin-newsom-win-the-2028-democratic-presidential-nomination-568']}},
  ts:{module:'../../packages/safe-cex/src/exchanges/polymarket/polymarket.exchange.ts',className:'PolymarketExchange',name:'polymarket'},
  matchRequestBody:true,
  compareWriteOrder:true,
  createSocketAudit:createPolymarketSocketAudit,
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
  verifyPublicRequest:true,
  requestBody:(body,{url,method})=>method.toUpperCase()==='POST'&&new URL(url).pathname==='/order'?{...body,order:{...body.order,salt:'<random>',timestamp:'<time>',signature:'<verified>'}}:body,
  requestKey(method,url,body,callerIds){return requestKey(method,url,body,callerIds,this);},
  subscriptionMessage(message){
    if (message.type==='market'||message.operation) return {op:message.operation??'subscribe',args:message.assets_ids.map(id=>({asset_id:id,custom_feature_enabled:message.custom_feature_enabled===true}))};
    return message;
  },
};
