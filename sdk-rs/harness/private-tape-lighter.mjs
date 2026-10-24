// Documented Lighter perps payloads, public offline scalar key, no live credentials.
// https://apidocs.lighter.xyz/docs/websocket-reference and checked-in SDK tx schema.
import { mkdirSync, writeFileSync } from 'node:fs';
import { dirname } from 'node:path';
import { parseArgs } from 'node:util';
const {values}=parseArgs({options:{venue:{type:'string',default:'lighter'},testnet:{type:'boolean',default:false},out:{type:'string'}}});
if (!['lighter','lighter-robinhood'].includes(values.venue)) throw new Error('Expected lighter or lighter-robinhood');
const robinhood=values.venue==='lighter-robinhood';
const REST=robinhood ? `https://api.rh${values.testnet?'-testnet':''}.lighter.xyz` : values.testnet?'https://testnet-api.lighter.xyz':'https://mainnet.zklighter.elliot.ai';
const WS=robinhood?REST.replace('https:','wss:')+'/stream':values.testnet?'wss://testnet.zklighter.elliot.ai/stream':'wss://mainnet.zklighter.elliot.ai/stream';
const out=values.out??`sdk-rs/harness/tapes/${values.venue}${values.testnet?'-testnet':''}-private.jsonl`;
// TS attributes Robinhood to 47296 on both networks, zkLighter only on mainnet (158385).
const integrator=robinhood?47296:158385;
const attributed=robinhood||!values.testnet;
const EPOCH=1792454400000;
const records=[{t:0,type:'meta',venue:values.venue,private:true,epoch:EPOCH},{t:0,type:'config',private:true,options:{testnet:values.testnet,key:'',secret:'01'+'00'.repeat(39),applicationId:'42',apiKeyIndex:'3'}}];
let time=0;
const http=(method,path,body,requestBody,extra={})=>records.push({t:0,type:'http',method,url:REST+path,status:200,body:JSON.stringify(body),...(requestBody===undefined?{}:{requestBody}),...extra});
const ws=(socket,message,t)=>{time=t??time+400;records.push({t:time,type:'ws',socket,url:WS,text:JSON.stringify(message)});};
const command=(body)=>{time+=400;records.push({t:time,type:'command',...body});};
const market=(market_id,symbol,size,price)=>({market_id,symbol,status:'active',market_type:'perp',supported_size_decimals:size,supported_price_decimals:price,min_base_amount:'0.001',min_quote_amount:'1',order_quote_limit:'1000000',min_initial_margin_fraction:500,maintenance_margin_fraction:250,market_config:{market_margin_mode:0}});
http('GET','/api/v1/orderBookDetails',{code:200,order_book_details:[market(1,'ETH',3,2),market(2,'BTC',4,1)]});
const stats=[{market_id:1,symbol:'ETH',last_trade_price:'100',mark_price:'100',index_price:'100',best_bid_price:'99',best_ask_price:'101',current_funding_rate:'0.0012',funding_rate:'0.0008',daily_base_token_volume:100,daily_quote_token_volume:10000},{market_id:2,symbol:'BTC',last_trade_price:'200',mark_price:'200',index_price:'200',best_bid_price:'199',best_ask_price:'201',daily_base_token_volume:50,daily_quote_token_volume:10000}];
http('GET','/api/v1/exchangeStats',{code:200,order_book_stats:stats});
const position=(leverage=10,margin_mode=0)=>({market_id:1,symbol:'ETH',position:'2',sign:1,avg_entry_price:'100',unrealized_pnl:'0',realized_pnl:'0',initial_margin_fraction:10000/leverage,margin_mode,liquidation_price:'50'});
const account=(leverage=10,margin_mode=0)=>({code:200,accounts:[{index:42,collateral:'1000',total_asset_value:'1000',available_balance:'980',positions:[position(leverage,margin_mode)],approved_integrators:[{account_index:integrator,approval_expiry:EPOCH+365*86400_000,max_perps_maker_fee:10000,max_perps_taker_fee:10000,max_spot_maker_fee:0,max_spot_taker_fee:0}]}]});
http('GET','/api/v1/account?by=index&value=42',account());
http('GET','/api/v1/accountLimits?account_index=42',{code:200,user_tier_name:'premium',user_tier:1});
http('GET','/api/v1/nextNonce?account_index=42&api_key_index=3',{code:200,nonce:7});
const row=(id,extra={})=>({order_index:String(id),order_id:String(id),client_order_index:id,market_id:1,symbol:'ETH',status:'open',type:'limit',time_in_force:1,is_ask:false,initial_base_amount:'0.25',filled_base_amount:'0',remaining_base_amount:'0.25',price:'90',reduce_only:false,updated_at:EPOCH+time,...extra});
const open=(orders,afterCommand=0,availableAt=0)=>http('GET','/api/v1/accountActiveOrders?account_index=42&market_id=255',{code:200,orders},undefined,{afterCommand,availableAt});
open([row(7001)]);
const orderInfo=(price,amount,type=0,tif=1,ask=0,reduce=0,trigger=0)=>({AccountIndex:42,ApiKeyIndex:3,MarketIndex:1,ClientOrderIndex:123,BaseAmount:amount,Price:price,IsAsk:ask,Type:type,TimeInForce:tif,ReduceOnly:reduce,TriggerPrice:trigger,OrderExpiry:1,ExpiredAt:1,Nonce:0,Sig:'placeholder',...(attributed?{L2TxAttributes:{1:integrator,2:250,3:0}}:{})});
const write=(type,info,nonce)=>http('POST','/api/v1/sendTx',{code:200,tx_hash:'offline-tx-'+nonce},new URLSearchParams({tx_type:String(type),tx_info:JSON.stringify({...info,Nonce:nonce})}).toString());
const cancel=(id,nonce)=>write(15,{AccountIndex:42,ApiKeyIndex:3,MarketIndex:1,Index:id,ExpiredAt:1,Sig:'placeholder'},nonce);
const setting=(fraction,mode,nonce)=>write(20,{AccountIndex:42,ApiKeyIndex:3,MarketIndex:1,InitialMarginFraction:fraction,MarginMode:mode,ExpiredAt:1,Sig:'placeholder'},nonce);
ws(0,{type:'update/market_stats',channel:'market_stats:all',market_stats:Object.fromEntries(stats.map(s=>[s.market_id,s]))});
ws(1,{type:'subscribed/account_all',channel:'account_all:42'});
ws(1,{type:'subscribed/account_all_orders',channel:'account_all_orders:42'});
ws(1,{type:'update/account_all_orders',channel:'account_all_orders:42',orders:{1:[row(7001)]}});
command({op:'cancelOrders',ids:['7001']}); cancel(7001,7);open([],1);
command({op:'placeOrders',orders:[{symbol:'ETH',type:'limit',side:'buy',price:90,amount:0.25,timeInForce:'PostOnly'}]});write(14,orderInfo(9000,250,0,2),8);
command({op:'setLeverage',symbol:'ETH',leverage:5});setting(2000,0,9);http('GET','/api/v1/account?by=index&value=42',account(5,0),undefined,{afterCommand:3});
command({op:'setMarginType',symbol:'ETH',isolated:true});setting(2000,1,10);http('GET','/api/v1/account?by=index&value=42',account(5,1),undefined,{afterCommand:4});
command({op:'setPositionMode',hedged:false});
command({op:'placeOrders',orders:[{symbol:'ETH',type:'limit',side:'buy',price:90,amount:0.25,stopLoss:80,takeProfit:120}]});
write(14,orderInfo(9000,250),11);write(14,orderInfo(7920,250,2,0,1,1,8000),12);write(14,orderInfo(11880,250,4,0,1,1,12000),13);
ws(1,{type:'update/account_all_orders',channel:'account_all_orders:42',orders:{1:[row(7002)]}});open([row(7002)],6,time);
command({op:'amendOrder',id:'7002',price:95,amount:0.5});cancel(7002,14);write(14,orderInfo(9500,500),15);open([],7);
command({op:'placeOrders',orders:[{symbol:'ETH',type:'limit',side:'buy',price:96,amount:0.25},{symbol:'ETH',type:'limit',side:'buy',price:97,amount:0.5}]});
http('POST','/api/v1/sendTxBatch',{code:200,tx_hash:['offline-tx-16','offline-tx-17']},new URLSearchParams({tx_types:'[14,14]',tx_infos:JSON.stringify([JSON.stringify({...orderInfo(9600,250),Nonce:16}),JSON.stringify({...orderInfo(9700,500),Nonce:17})])}).toString());
// The legacy array payload makes TS emit both a FILLED order and the execution.
// Rust accepts the execution once; the independent gate oracle removes the TS duplicate.
const filled=row(8001,{status:'filled',price:'100',initial_base_amount:'0.25',filled_base_amount:'0.25',remaining_base_amount:'0',updated_at:EPOCH+6500});
ws(1,{type:'update/account_all',channel:'account_all:42',data:{orders:[filled],fills:[{fillId:'execution-1',orderId:'8001',symbol:'ETH',side:'BUY',price:'100',size:'0.25',timestamp:EPOCH+6500}]}},6500);
mkdirSync(dirname(out),{recursive:true});writeFileSync(out,records.map(r=>JSON.stringify(r)).join('\n')+'\n');
console.log(JSON.stringify({out,events:records.filter(r=>['ws','command'].includes(r.type)).length,commands:records.filter(r=>r.type==='command').length}));
