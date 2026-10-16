const load=require('./load-module.cjs');
const unavailable=new Proxy(function(){throw Error('Unselected dependency invoked')},{get(_,key){if(key==='__esModule')return false;return ()=>{throw Error('Unselected dependency invoked '+String(key))}}});
let fixture,calls;
const registry={getTransport:options=>{calls.push(['getTransport',options]);return {privateProxy:exchange=>{calls.push(['privateProxy',exchange]);return fixture.proxy?{get url(){calls.push(['url']);return fixture.url},resolveHeaders:async()=>{calls.push(['resolveHeaders']);if(fixture.error)throw Error(fixture.error);return fixture.headers}}:undefined;}}}};
const approval=load('packages/safe-cex/src/exchanges/asterdex/asterdex.builder-approval.ts',['buildAsterApprovalRequest'],{ethers:unavailable,'../../transport/registry':registry,'./asterdex.api':unavailable,'./asterdex.builder-fee':unavailable,'./asterdex.types':require('./source.cjs').values});
(async()=>{const cases=[];for(const base of [undefined,'',' ','https://override'])for(const proxy of [false,true])for(const headers of [{},{Authorization:'token'}, {Authorization:''}])for(const error of [undefined,'header failure']){
 fixture={proxy,url:'https://proxy/',headers,error};calls=[];let output;try{output={value:await approval.buildAsterApprovalRequest('https://venue/path',base)}}catch(e){output={error:e.message}}cases.push({base,fixture,output,calls});
}process.stdout.write(JSON.stringify(cases));})().catch(error=>{console.error(error);process.exitCode=1});
