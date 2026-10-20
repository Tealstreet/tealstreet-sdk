// Independent Node crypto oracle for the Futures wire; never use the Rust signer.
import {createHash,createHmac} from 'node:crypto';
const KEY='replay-key';
const SECRET=Buffer.from('replay-secret');
export const krakenSignature=(payload)=>createHmac('sha512',SECRET).update(createHash('sha256').update(payload).digest()).digest('base64');
export function createKrakenRequestAudit({strict=true}={}){
 let previous=0n;const minted=new Set();
 return ({method,url,headers,body,now=Date.now()})=>{
  const u=new URL(url);if(u.protocol!=='https:'||!['futures.kraken.com','demo-futures.kraken.com'].includes(u.hostname))throw Error('Kraken request must use upstream host');
  const entries=(Array.isArray(headers)?headers:Object.entries(headers??{})).map(([k,v])=>[k.toLowerCase(),String(v)]);
  if(new Set(entries.map(([k])=>k)).size!==entries.length)throw Error('Kraken duplicate request header');
  const h=Object.fromEntries(entries);
  if((method==='GET'&&/\/v3\/(instruments|tickers|orderbook|history)$/.test(u.pathname))||u.pathname.startsWith('/api/charts/')){
   if(h.apikey||h.authent||h.nonce)throw Error('Kraken public request leaked credentials');return;
  }
  const n=h.nonce;if(h.apikey!==KEY||!/^\d+$/.test(n??''))throw Error('Kraken missing authentication');
  const nonce=BigInt(n);if((strict&&nonce<=previous)||Math.abs(Number(nonce/1000n)-now)>60_000)throw Error('Kraken nonce not fresh and increasing');previous=nonce>previous?nonce:previous;
  const path=u.pathname.replace(/^\/derivatives/,'');
  const bytes=method==='GET'?u.search.slice(1):body??'';
  if(typeof bytes!=='string')throw Error('Kraken signed body must be encoded bytes');
  const params=new URLSearchParams(bytes),names=[...params.keys()];
  if(new Set(names).size!==names.length)throw Error('Kraken duplicate form parameter');
  if(params.has('json'))uniqueJson(params.get('json'));
  if(h.authent!==krakenSignature(bytes+n+path))throw Error('Kraken signature does not cover sent encoded form/path');
  if(u.pathname==='/derivatives/api/v3/sendorder'){
   const id=new URLSearchParams(bytes).get('cliOrdId');
   if(!/^ts_[A-Za-z0-9]{1,21}$/.test(id??'')||minted.has(id))throw Error('Kraken minted client ID missing, invalid or reused');
   minted.add(id);
  }
 };
}
export function validateKrakenSocketMessage(socket,text){
 const m=JSON.parse(text);
 if(m.event==='challenge'){
  if(m.api_key!==KEY)throw Error('Kraken challenge key mismatch');socket.krakenWaiting=true;delete socket.krakenChallenge;socket.krakenFeeds=new Set();return;
 }
 if(!['subscribe','unsubscribe'].includes(m.event)||!['fills','open_orders','open_positions','account_balances'].includes(m.feed))return;
 // account_balances is TS's known invalid feed; it never supplies data.
 if(m.feed==='account_balances')return;
 if(socket.krakenWaiting||!socket.krakenChallenge||m.api_key!==KEY||m.original_challenge!==socket.krakenChallenge||m.signed_challenge!==krakenSignature(socket.krakenChallenge))throw Error('Kraken private subscription lacks a current signed challenge');
 socket.krakenFeeds??=new Set();if(m.event==='subscribe')socket.krakenFeeds.add(m.feed);else socket.krakenFeeds.delete(m.feed);
}
export function validateKrakenSocketFrame(socket,text){
 const m=JSON.parse(text);
 if(m.event==='challenge'){
  if(!socket.krakenWaiting||!m.message)throw Error('Kraken unsolicited challenge');socket.krakenWaiting=false;socket.krakenChallenge=m.message;return;
 }
 const feed=m.feed?.replace(/_snapshot$/,'');
 if(['fills','open_orders','open_positions'].includes(feed)&&!m.event&&!socket.krakenFeeds?.has(feed))throw Error('Kraken data arrived before authenticated subscription');
}
export function krakenRequestBody(body,{callerIds=new Set()}={}){
 // Preserve the sent spelling of unsafe integer-valued JSON numbers, including
 // decimal/exponent forms, rather than letting Number round native identities.
 const row=body?.json?JSON.parse(body.json, (_key,value,context)=>
  typeof value==='number'&&Number.isInteger(value)&&!Number.isSafeInteger(value)
   ?JSON.rawJSON(context.source):value):body;
 const walk=v=>JSON.isRawJSON(v)?v:Array.isArray(v)?v.map(walk):v&&typeof v==='object'?Object.fromEntries(Object.entries(v).map(([k,x])=>[k,k==='cliOrdId'&&typeof x==='string'&&x.startsWith('ts_')&&!callerIds.has(x)?'<generated>':walk(x)])):v;
 return walk(row);
}
export function krakenCommandResult(result,command){
 if(command.op==='placeOrders'&&result.error&&/caller.*client|client.*IDs/.test(result.error))return {error:'caller client order ID unsupported'};
 if(command.op==='amendOrder'&&result.error&&/trailing/.test(result.error))return {error:'trailing amendment unsupported'};
 // TS's inherited setter rejects even the venue's only supported mode.
 if(command.op==='setPositionMode'&&command.hedged===false){
  if(result.error==='Not implemented'||('ok'in result&&(result.ok===null||Array.isArray(result.ok)&&result.ok.length===0)))return {ok:[]};
 }
 return result;
}

// JSON.parse validates syntax; retain object scopes to reject repeated keys
// before a last-wins parser can hide a signed batch's economic fields.
function uniqueJson(text){
 JSON.parse(text);
 const tokens=text.match(/"(?:\\.|[^"\\])*"|[{}\[\]:,]/g)??[];const scopes=[];
 for(let i=0;i<tokens.length;i++){
  const t=tokens[i];if(t==='{')scopes.push(new Set());else if(t==='[')scopes.push(null);else if(t==='}'||t===']')scopes.pop();
  else if(t.startsWith('"')&&tokens[i+1]===':'){
   const key=JSON.parse(t),keys=scopes.at(-1);if(keys?.has(key))throw Error('Kraken duplicate JSON property');keys?.add(key);
  }
 }
}
