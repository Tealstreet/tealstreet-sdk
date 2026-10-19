// Execute maintained classes; only the terminal key-value I/O is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const { root, load } = require('../../safe-cex-types/scripts/accounts-source.cjs');
const { CandleCache, cleanCandles } = load(path.join(root, 'packages/safe-cex/src/candleCache/CandleCache.ts'));
const { KeyValueCandleCacheBackend } = load(path.join(root, 'packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts'));
const c = load(path.join(root, 'packages/safe-cex/src/utils/calendarCandles.ts'));
const intervals = ['1s','5s','15s','30s','1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','3h','5h','7h','8h','9h','10h','11h','24h','2d','3d','1M'];
const candle = timestamp => ({ timestamp, open:11, high:17, low:7, close:13, volume:19 });
const terminal = (values, reads = []) => ({ get:async k => { reads.push(k); return values.get(k); }, set:async(k,v)=>values.set(k,v), delete:async k=>values.delete(k), listKeys:async p=>[...values.keys()].filter(k=>k.startsWith(p)) });
(async () => {
  if (process.argv.includes('--seeded-clean-candles')) {
    const bits = value => { const b=Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
    const pack = row => ['timestamp','open','high','low','close','volume'].map(key=>bits(row[key]));
    const value = timestamp => ({timestamp,open:100,high:110,low:90,close:105,volume:1000});
    const firstNan={...value(NaN),close:777};
    const cases=[[...Array.from({length:5},(_,i)=>value(10-i)),firstNan,...Array.from({length:5},(_,i)=>value(5-i)),value(NaN)],[NaN,Infinity,-0,-Infinity,0].map(value)];
    let seed=0x123456789abcdef0n;
    for(let c=0;c<2000;c++) {
      const input=[{...value(NaN),close:c}];
      for(let i=0;i<64;i++) {
        seed=BigInt.asUintN(64,seed*6364136223846793005n+1n);
        if(seed%7n===0n) input.push(value(NaN));
        else input.push({...value(Number((seed>>32n)%81n)-40),close:i});
      }
      cases.push(input);
    }
    assert.equal(cases.length,2002,'legacy seeded source floor');
    process.stdout.write(JSON.stringify(cases.map(rows=>cleanCandles(rows).map(pack))));
    return;
  }
  if (process.argv.includes('--clean-candles')) {
    const bits = value => { const b=Buffer.alloc(8); b.writeDoubleBE(value); return b.toString('hex'); };
    const pack = row => ['timestamp','open','high','low','close','volume'].map(key=>bits(row[key]));
    const inputs = [[], [NaN,Infinity,-0,-Infinity,0], [10,9,8,7,6,NaN,5,4,3,2,1,NaN]];
    let seed=0x9abcdef0;
    for (const length of [1,2,3,31,32,63,64,65,127,128,257,511,1024]) {
      for(let variant=0;variant<10;variant++) {
        const values=[];
        for(let i=0;i<length;i++) {
          seed=(Math.imul(seed,1664525)+1013904223)>>>0;
          let value=variant%3===0?length-i:variant%3===1?i:(seed%2049)-1024;
          if(i%17===variant) value=NaN;
          if(i%29===variant) value=Infinity;
          if(i%31===variant) value=-Infinity;
          if(i%43===variant) value=-0;
          if(i%47===variant) value=0;
          values.push(value);
        }
        inputs.push(values);
      }
    }
    const profiles=[];
    for(const [id,values] of inputs.entries()) {
      const input=values.map((timestamp,i)=>({timestamp,open:i+11,high:i+17,low:i+7,close:i+13,volume:i+19}));
      const expected=cleanCandles(input);
      const cache=new CandleCache();
      const fetched=await cache.fetch({cacheKey:'sort',interval:'1m',fromMs:1699999980000,toMs:1699999980000,limit:1,nowMs:1700000040000,usePersistentBackend:false,fetcher:async()=>input});
      assert.deepEqual(fetched.map(pack),expected.map(pack),'real source fetch uses cleanCandles');
      profiles.push({id,input:input.map(pack),expected:expected.map(pack)});
    }
    assert.equal(profiles.length,133,'independent cleanCandles profile count');
    process.stdout.write(JSON.stringify(profiles));
    return;
  }
  const rows = [];
  for (const interval of intervals) {
    const start = c.chartCandleStartTime(interval,1706745600);
    const next = c.nextChartCandleStartTime(interval,start);
    const third = c.nextChartCandleStartTime(interval,next);
    const now = c.nextChartCandleStartTime(interval,third);
    const to = c.nextChartCandleStartTime(interval,now);
    const boundaries = [!!c.calendarPeriodForInterval(interval), start, next, c.chartCandleCloseTime(interval,start), c.getNumChartCandlesInRange(interval,start,third)];
    assert.equal(boundaries[4],3,'inclusive three candles');
    for (const mode of ['read-through','hydrate-once']) {
      const values = new Map(), backend = new KeyValueCandleCacheBackend(terminal(values));
      let requests = 0;
      const fetcher = async () => { requests++; return [start,next,third].map(t=>candle(t*1000)); };
      const request = { cacheKey:interval, interval, fromMs:start*1000, toMs:third*1000, limit:3, nowMs:now*1000, fetcher };
      const first = new CandleCache({backend,backendReadMode:mode});
      assert.deepEqual(await first.fetch(request),[start,next,third].map(t=>candle(t*1000)));
      // Await detached KV Promise continuations, without advancing wall time.
      await new Promise(setImmediate);
      const fresh = new CandleCache({backend:new KeyValueCandleCacheBackend(terminal(values)),backendReadMode:mode});
      const result = await fresh.fetch(request);
      assert.equal(requests,1,'reopened backend must serve all three candles');
      const lookup = fresh.getCachedCandles(interval,interval,start*1000,to*1000,5,now*1000);
      rows.push({interval,mode,boundaries,timestamps:result.map(r=>r.timestamp),lookup:{behind:lookup.contiguousBehindCandles.map(r=>r.timestamp),forward:lookup.contiguousForwardCandles.map(r=>r.timestamp),from:lookup.from,to:lookup.to,limit:lookup.limit}});
    }
  }
  assert.equal(rows.length,54);
  // Pin the exact remaining key namespace / integer differences, not just success.
  const reads=[], prefix='tealstreet-candles::keys::';
  const listed = [prefix+'9007199254740993',prefix+'9223372036854775808','other::0000000000000000000012',prefix+'\ufeff12tail',prefix+'junk'];
  const backend=new KeyValueCandleCacheBackend({...terminal(new Map(),reads),listKeys:async()=>listed});
  await backend.loadAllChunks('keys',{chunkSize:1,intervalSeconds:1});
  assert.deepEqual(reads,[prefix+'9007199254740992',prefix+'9223372036854776000',prefix+'12',prefix+'12']);
  const values=new Map(); const negatives=new KeyValueCandleCacheBackend(terminal(values));
  const slots=[];slots[-1]=candle(-60000);
  await negatives.persistChunks('negative',new Map([[-1,slots]]));
  assert.equal(values.size,0,'source filter drops negative array properties');
  process.stdout.write(JSON.stringify({rows,keyReads:reads,negativeWrites:values.size}));
})().catch(e=>{console.error(e);process.exitCode=1;});
