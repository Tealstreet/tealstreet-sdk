// Execute maintained classes; only the terminal key-value I/O is scripted.
const assert = require('node:assert/strict');
const path = require('node:path');
const { root, load } = require('../../safe-cex-types/scripts/accounts-source.cjs');
const { CandleCache } = load(path.join(root, 'packages/safe-cex/src/candleCache/CandleCache.ts'));
const { KeyValueCandleCacheBackend } = load(path.join(root, 'packages/safe-cex/src/candleCache/backends/KeyValueCandleCacheBackend.ts'));
const c = load(path.join(root, 'packages/safe-cex/src/utils/calendarCandles.ts'));
const intervals = ['1s','5s','15s','30s','1m','3m','5m','15m','30m','1h','2h','4h','6h','12h','1d','1w','3h','5h','7h','8h','9h','10h','11h','24h','2d','3d','1M'];
const candle = timestamp => ({ timestamp, open:11, high:17, low:7, close:13, volume:19 });
const terminal = (values, reads = []) => ({ get:async k => { reads.push(k); return values.get(k); }, set:async(k,v)=>values.set(k,v), delete:async k=>values.delete(k), listKeys:async p=>[...values.keys()].filter(k=>k.startsWith(p)) });
(async () => {
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
