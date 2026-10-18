// Whole maintained venue/Base/Store/API classes; only terminal Axios I/O is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const {root, load, external} = require('./inverse-source-loader.cjs');
const {AxiosError} = external('axios');
const {DefaultStore} = load(path.join(root, 'packages/safe-cex/src/store/store.base.ts'));
const {setTelemetrySink} = load(path.join(root, 'packages/safe-cex/src/telemetry.ts'));
const units = text => Array.from({length:text.length}, (_, i) => text.charCodeAt(i));
const bits = number => {const buffer=Buffer.alloc(8);buffer.writeDoubleBE(number);return buffer.toString('hex')};
(async () => {
  const outputs=[];
  for (const [stem,name] of [['binance','BinanceExchange'],['binancepm','BinancePMExchange']]) {
    const Subject=load(path.join(root, `packages/safe-cex/src/exchanges/binance/${stem}.exchange.ts`))[name];
    for (const [profile,msg,expected] of [
      ['object',{name:'VenueError',message:'raw venue reason',detail:17},'raw venue reason'],
      ['number',7,'7'],
      ['string','plain venue reason','Exchange Error: plain venue reason'],
    ]) {
      const store=new DefaultStore();
      const venue=new Subject(stem,{key:'synthetic-key',secret:'synthetic-secret',tuning:{consoleLogging:false}},store);
      store.update({markets:[{id:'BTCUSDT',symbol:'BTCUSDT',base:'BTC',quote:'USDT',active:true,precision:{amount:.1,price:.1},limits:{amount:{min:.1,max:100},leverage:{min:1,max:50}}}],positions:[{id:'witness-position',symbol:'BTCUSDT',side:'long',entryPrice:100,notional:100,contracts:1,leverage:10,unrealizedPnl:0,liquidationPrice:0}]});
      const events=[],reports=[],consoleErrors=[];
      venue.on('error',value=>events.push(value));
      setTelemetrySink({captureException:error=>reports.push(error)});
      const originals={error:console.error,trace:console.trace,warn:console.warn};
      console.error=(...args)=>consoleErrors.push(args);console.trace=()=>{};console.warn=()=>{};
      let requests=0;
      try {
        for (const client of new Set([venue.xhr,venue.papiXhr,venue._primaryXhr,venue._secondaryXhr,venue.publicXhr,venue.unlimitedXHR,venue.fapiXhr].filter(Boolean))) {
          client.defaults.adapter=async config=>{
            requests++;
            assert.equal(config.method,'post');
            assert.match(config.params.signature,/^[a-f0-9]{64}$/);
            assert.equal(config.headers.get('X-MBX-APIKEY'),'synthetic-key');
            const response={status:400,statusText:'Bad Request',headers:{'content-type':'application/json'},config,data:JSON.stringify({msg})};
            throw new AxiosError('Request failed with status code 400','ERR_BAD_REQUEST',config,undefined,response);
          };
        }
        await venue.setLeverage('BTCUSDT',20);
        assert.equal(events.at(-1),expected);
        assert.equal(requests,1);
        assert.equal(store.positions[0].leverage,10);
        if (profile==='object') {
          const reported=reports.at(-1);
          assert.equal(reported instanceof Error,false,'source preserves the JSON object rather than wrapping it');
          assert.equal(reported.name,'VenueError');assert.equal(reported.message,'raw venue reason');assert.equal(reported.detail,17);
          assert.equal(reported,consoleErrors.at(-1)[1],'source console and telemetry receive the same raw object');
        }
        outputs.push({stem,profile,messageUnits:units(events.at(-1)),requests,leverageBits:bits(store.positions[0].leverage)});
      } finally {setTelemetrySink({});Object.assign(console,originals);venue.dispose();}
    }
  }
  assert.equal(outputs.length,6,'two maintained families by three raw-message profiles');
  process.stdout.write(JSON.stringify(outputs));
})().catch(error=>{console.error(error);process.exitCode=1});
