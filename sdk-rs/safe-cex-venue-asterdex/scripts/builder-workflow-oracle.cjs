// Unmodified source workflows; wallet, fetch, transport and clock are host seams.
const fs=require('node:fs'),path=require('node:path'),load=require('./load-module.cjs'),{values}=require('./source.cjs'),api=require('./api-source.cjs');
const compiler=path.resolve(path.dirname(require.resolve(process.env.SAFE_CEX_TYPESCRIPT_PATH)),'..'),ethers=require(path.join(path.dirname(compiler),'ethers'));
const unavailable=new Proxy(function(){throw Error('Unselected dependency invoked');},{get(_,key){if(key==='__esModule')return false;return ()=>{throw Error('Unselected dependency invoked '+String(key))}}});
const fees=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-fee.ts',[],{ethers,'../../types':{OrderType:{},OrderTimeInForce:{}}});
const approval=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts',['switchToAsterSignatureChain'],{ethers,'../../transport/registry':{getTransport:()=>({privateProxy:()=>undefined})},'./asterdex.api':api,'./asterdex.builder-fee':fees,'./asterdex.types':values});
const cases=JSON.parse(fs.readFileSync(path.join(__dirname,'../tests/builder-workflow-cases.json'),'utf8'));
const error=q=>{if(q.isError)return Object.assign(Error(q.reject.message||''),q.reject);return q.reject;};
(async()=>{const out=[];try{for(let i=0;i<cases.length;i++){const c=cases[i],walletCalls=[],providerCalls=[],fetchCalls=[],walletQueue=[...(c.wallet||[])],fetchQueue=[...(c.fetch||[])];
 api.setOracleClock(1000000+i*2000);
 const provider=id=>({request:async request=>{walletCalls.push(request);providerCalls.push(id);if(!walletQueue.length)throw Error('Unplanned wallet call');const q=walletQueue.shift();if(q.replaceProvider)global.window.ethereum=provider(1);if('reject'in q)throw error(q);return q.value;}});if(c.noWallet)delete global.window;else global.window={ethereum:provider(0)};
 global.fetch=async(url,opts)=>{fetchCalls.push({url,method:opts.method||'GET',headers:opts.headers,body:opts.body===undefined?null:String(opts.body)});if(!fetchQueue.length)throw Error('Unplanned fetch');const q=fetchQueue.shift();if('reject'in q)throw error(q);return{ok:q.ok??true,status:q.status??200,json:async()=>{if(q.jsonError)throw Error(q.jsonError);return q.payload;}};};
 let result,rejection;try{result=await (c.kind==='switch'?approval.switchToAsterSignatureChain():c.kind==='check'?approval.checkAsterBuilderApproved(c.params):approval.approveAsterBuilderFeeWithBrowserWallet(c.params));}catch(e){rejection=e.message;}
 out.push({...c,index:i,result,rejection,walletCalls,providerCalls,fetchCalls});
 }}finally{delete global.window;delete global.fetch;}process.stdout.write(JSON.stringify(out));})().catch(e=>{process.stderr.write(e.stack);process.exitCode=1;});
