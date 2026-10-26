import { readFileSync } from 'node:fs';
import { join } from 'node:path';

import { describe, expect, it } from 'vitest';

import { WOO_V3_MIGRATION_GATES } from '../reference/woo/woo.v3.contracts';
import { prepareWooV3Request } from '../reference/woo/woo.v3.request';
import {
  WOO_V3_WS_BLOCKERS,
  WOO_V3_WS_CHANNELS,
  WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS,
  WOO_V3_WS_HEARTBEAT,
  WOO_V3_WS_LIMITS,
  WOO_V3_WS_PRIVATE_TOPICS,
  WOO_V3_WS_PUBLIC_TOPIC_KINDS,
  acknowledgeWooV3WsSubscription,
  advanceWooV3OrderBookSequence,
  buildWooV3ListenKeyRequest,
  buildWooV3PrivateTopic,
  buildWooV3PrivateWebSocketUrl,
  buildWooV3PublicTopic,
  buildWooV3Subscribe,
  buildWooV3WsPing,
  createWooV3WsSubscriptionState,
  decodeWooV3ListenKey,
  decodeWooV3WsCommandAck,
  decodeWooV3WsDataFrame,
  decodeWooV3WsPong,
  parseWooV3WsTopic,
  planWooV3WsResubscribe,
  reconnectWooV3WsSubscription,
} from '../reference/woo/woo.v3.websocket';

const auth = {
  apiKey: 'fixture-key',
  apiSecret: 'fixture-secret',
  timestamp: 1_750_641_291_182,
} as const;

const publicFrame = (topic: string, data: unknown) => ({ topic, ts: 1_758_820_361_552, data });
const privateFrame = (topic: string, data: unknown) => ({ topic, ts: 1_716_861_043_337, data });
const EXTREME_SMALL_DECIMAL = `0.${'0'.repeat(400)}1`;

describe('WOO V3 inert websocket contracts', () => {
  it('locks the official channels, listen-key descriptor, and existing V3 REST signature contract', () => {
    expect(WOO_V3_WS_CHANNELS).toEqual({
      public: {
        livenetUrl: 'wss://wss.woox.io/v3/public',
        testnetUrl: 'wss://wss.staging.woox.io/v3/public',
        authentication: 'none',
      },
      private: {
        livenetUrl: 'wss://wss.woox.io/v3/private',
        testnetUrl: 'wss://wss.staging.woox.io/v3/private',
        authentication: 'listen-key-query',
        listenKeyEndpoint: '/v3/account/listenKey',
      },
    });
    const request = buildWooV3ListenKeyRequest();
    expect(request).toEqual({ method: 'POST', path: '/v3/account/listenKey', body: { type: 'WEBSOCKET' } });
    expect(prepareWooV3Request(request, auth)).toMatchObject({
      method: 'POST',
      path: '/v3/account/listenKey',
      body: '{"type":"WEBSOCKET"}',
      signaturePayload: '1750641291182POST/v3/account/listenKey{"type":"WEBSOCKET"}',
      headers: {
        'x-api-key': 'fixture-key',
        'x-api-timestamp': '1750641291182',
        'content-type': 'application/json',
      },
    });
    const listenKey = decodeWooV3ListenKey({
      success: true,
      timestamp: 1_750_641_291_182,
      data: { authKey: 'a/b+c', expiredTime: 1_750_727_691_182 },
    });
    expect(listenKey.authKey).toBe('a/b+c');
    expect(buildWooV3PrivateWebSocketUrl('livenet', listenKey.authKey)).toBe(
      'wss://wss.woox.io/v3/private?key=a%2Fb%2Bc',
    );
    expect(() =>
      decodeWooV3ListenKey({
        success: true,
        timestamp: 100,
        data: { authKey: '', expiredTime: 100 },
      }),
    ).toThrow();
  });

  it('inventories and builds only documented public/private topics', () => {
    expect(WOO_V3_WS_PUBLIC_TOPIC_KINDS).toHaveLength(12);
    expect(WOO_V3_WS_PRIVATE_TOPICS).toEqual([
      'account',
      'balance',
      'position',
      'executionreport',
      'algoexecutionreport',
    ]);
    expect(buildWooV3PublicTopic('orderbookupdate', 'PERP_BTC_USDT', 50)).toMatchObject({
      topic: 'orderbookupdate@PERP_BTC_USDT@50',
      depth: 50,
      includesRpi: false,
    });
    expect(buildWooV3PublicTopic('orderbookupdaterpi', 'SPOT_BTC_USDT', 500)).toMatchObject({
      topic: 'orderbookupdaterpi@SPOT_BTC_USDT@500',
      includesRpi: true,
    });
    expect(buildWooV3PublicTopic('kline', 'PERP_ETH_USDT', '1M').topic).toBe('kline@PERP_ETH_USDT@1M');
    expect(buildWooV3PublicTopic('bbo', 'SPOT_BTC_USDT').topic).toBe('bbo@SPOT_BTC_USDT');
    expect(buildWooV3PrivateTopic('executionreport')).toEqual({
      channel: 'private',
      kind: 'executionreport',
      topic: 'executionreport',
    });
    expect(parseWooV3WsTopic('orderbookupdate@PERP_BTC_USDT@200')).toMatchObject({ depth: 200 });
    expect(() => buildWooV3PublicTopic('orderbookupdate', 'PERP_BTC_USDT', 100 as never)).toThrow('50, 200, or 500');
    expect(() => buildWooV3PublicTopic('kline', 'PERP_BTC_USDT', '2m' as never)).toThrow('documented');
    expect(() => parseWooV3WsTopic('ticker@BTCUSDT')).toThrow('documented WOO');
    expect(() => parseWooV3WsTopic('unknown@PERP_BTC_USDT')).toThrow('unknown');
  });

  it('locks subscribe acknowledgements, limits, reconnect state, and deterministic batches', () => {
    const topics = Array.from({ length: 21 }, (_, index) => `ticker@PERP_T${index}_USDT`);
    const state = createWooV3WsSubscriptionState('public', topics);
    const commands = planWooV3WsResubscribe(state, 'generation-0');
    expect(commands.map((command) => command.wire.params?.length)).toEqual([20, 1]);
    expect(commands.map((command) => command.wire.id)).toEqual(['generation-0-1', 'generation-0-2']);
    expect(commands.map((command) => command.channel)).toEqual(['public', 'public']);
    expect(WOO_V3_WS_COMMAND_ID_CONTRACT_STATUS).toEqual({
      emittedSubset: 'documented-response-string',
      blocker: WOO_V3_WS_BLOCKERS['command-request-id-type-undocumented'],
    });

    const firstAck = decodeWooV3WsCommandAck(
      {
        id: 'generation-0-1',
        cmd: 'SUBSCRIBE',
        success: true,
        time: 1_750_641_291_322,
        data: commands[0]!.wire.params,
      },
      commands[0],
    );
    const acknowledged = acknowledgeWooV3WsSubscription(state, commands[0]!, firstAck);
    expect(acknowledged.acknowledgedTopics).toHaveLength(20);
    const reconnected = reconnectWooV3WsSubscription(acknowledged);
    expect(reconnected).toMatchObject({ channel: 'public', generation: 1, acknowledgedTopics: [] });

    expect(WOO_V3_WS_LIMITS.maximumTopicsPerCommand).toBe(20);
    expect(WOO_V3_WS_LIMITS.maximumTopicsPerConnection).toBe(100);
    expect(() => buildWooV3Subscribe('public', 'too-many', [...topics, 'ticker@PERP_EXTRA_USDT'])).toThrow('1-20');
    expect(() => createWooV3WsSubscriptionState('public', Array(101).fill('ticker@PERP_BTC_USDT'))).toThrow();
    expect(() => createWooV3WsSubscriptionState('public', ['ticker@PERP_BTC_USDT', 'balance'])).toThrow(
      'private topic for a public websocket',
    );
    expect(() => buildWooV3Subscribe('private', 'mixed', ['balance', 'ticker@PERP_BTC_USDT'])).toThrow(
      'public topic for a private websocket',
    );
    const privateState = reconnectWooV3WsSubscription(createWooV3WsSubscriptionState('private', ['balance']));
    expect(privateState).toMatchObject({
      channel: 'private',
      generation: 1,
      intendedTopics: ['balance'],
      incrementalBooksRequireFreshSnapshot: false,
    });
    expect(() =>
      decodeWooV3WsCommandAck(
        { id: 'wrong', cmd: 'SUBSCRIBE', success: true, time: 1, data: commands[0]!.wire.params },
        commands[0],
      ),
    ).toThrow('identity');
    expect(() =>
      decodeWooV3WsCommandAck({ id: 'x', cmd: 'LIST_SUBSCRIPTION', success: true, time: 1, data: ['unknown'] }),
    ).toThrow('unknown WOO V3 websocket topic');
    expect(() =>
      decodeWooV3WsCommandAck({
        id: 'x',
        cmd: 'LIST_SUBSCRIPTION',
        success: true,
        time: 1,
        data: ['balance', 'ticker@PERP_BTC_USDT'],
      }),
    ).toThrow('mixes public and private');
    expect(() =>
      decodeWooV3WsCommandAck({ id: 'x', cmd: 'SUBSCRIBE', success: false, time: 1, data: [] }),
    ).toThrow('only success=true');
  });

  it('classifies documented PING/PONG as an optional latency probe rather than keepalive', () => {
    expect(WOO_V3_WS_HEARTBEAT).toEqual({
      classification: 'optional-latency-probe-not-keepalive',
      serverInitiatedPingDocumented: false,
      clientPingCommand: 'PING',
      serverPongCommand: 'PONG',
    });
    expect(buildWooV3WsPing(1_750_641_291_182)).toEqual({ cmd: 'PING', ts: 1_750_641_291_182 });
    expect(decodeWooV3WsPong({ cmd: 'PONG', success: true, time: 1_750_641_291_322 })).toEqual({
      cmd: 'PONG',
      success: true,
      time: 1_750_641_291_322,
    });
    expect(() => decodeWooV3WsPong({ cmd: 'ping', success: true, time: 1 })).toThrow('PONG');
  });

  it('strictly decodes public book, BBO, trade, ticker, kline, and futures scalar topics', () => {
    const update = decodeWooV3WsDataFrame(
      publicFrame('orderbookupdate@PERP_BTC_USDT@50', {
        s: 'PERP_BTC_USDT',
        prevTs: 1_618_826_337_380,
        asks: [
          ['56749.15', '3.92864'],
          ['56749.8', '0'],
        ],
        bids: [['56745.2', '1.03895025']],
        ts: 1_618_826_337_570,
      }),
    );
    expect(update).toMatchObject({ channel: 'public', blockers: [], payload: { previousTimestamp: 1_618_826_337_380 } });
    const payload = update.payload as { previousTimestamp: number; timestamp: number };
    expect(advanceWooV3OrderBookSequence(1_618_826_337_380, update.payload as never)).toEqual({
      accepted: true,
      nextExpectedPreviousTimestamp: payload.timestamp,
    });
    expect(advanceWooV3OrderBookSequence(1, update.payload as never)).toEqual({
      accepted: false,
      blocker: 'sequence-gap',
      expectedPreviousTimestamp: 1,
      actualPreviousTimestamp: payload.previousTimestamp,
      requiresFreshSnapshot: true,
    });

    expect(
      decodeWooV3WsDataFrame(
        publicFrame('orderbook10@PERP_SOL_USDT', {
          s: 'PERP_SOL_USDT',
          ts: 1_762_308_368_647,
          bids: [['154.36', '731.000']],
          asks: [['154.44', '230.000']],
        }),
      ).payload,
    ).toMatchObject({ symbol: 'PERP_SOL_USDT', bids: [['154.36', '731.000']] });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('bborpi@SPOT_BTC_USDT', {
          s: 'SPOT_BTC_USDT',
          ap: '42598.2',
          aq: '1.05',
          bp: '42598.1',
          bq: '2.01',
          ts: 1_618_820_361_540,
        }),
      ).payload,
    ).toMatchObject({ askPrice: '42598.2', bidPrice: '42598.1' });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('trade@SPOT_BTC_USDT', {
          s: 'SPOT_BTC_USDT',
          px: '42598.27',
          sx: '300',
          sd: 'BUY',
          src: 0,
          rpi: false,
          ts: 1_618_820_361_540,
        }),
      ).payload,
    ).toMatchObject({ side: 'BUY', source: 0, matchedRpi: false });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('ticker@SPOT_WOO_USDT', {
          s: 'SPOT_WOO_USDT',
          o: '0.16112',
          c: '0.32206',
          h: '0.33000',
          l: '0.14251',
          v: '89040821.98',
          a: '22493062.21',
          q: '89040821.98',
          u: '22493062.21',
          cnt: 15442,
          ts: 1_614_152_260_000,
          tts: 1_614_152_250_000,
        }),
      ).payload,
    ).toMatchObject({ close: '0.32206', tradeCount: 15442 });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('kline@SPOT_BTC_USDT@1m', {
          s: 'SPOT_BTC_USDT',
          t: '1m',
          o: '56948.97',
          c: '56891.76',
          h: '56948.97',
          l: '56889.06',
          v: '0',
          a: '0',
          st: 1_618_822_380_000,
          et: 1_618_822_440_000,
          ts: 1_614_152_260_000,
          tts: 1_614_152_250_000,
        }),
      ).payload,
    ).toMatchObject({ interval: '1m', startTimestamp: 1_618_822_380_000 });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('markprice@PERP_ETH_USDT', { s: 'PERP_ETH_USDT', px: '3987.2', ts: 1_618_820_361_552 }),
      ).payload,
    ).toEqual({ symbol: 'PERP_ETH_USDT', price: '3987.2', timestamp: 1_618_820_361_552 });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('markprice@PERP_ETH_USDT', {
          s: 'PERP_ETH_USDT',
          px: EXTREME_SMALL_DECIMAL,
          ts: 1_618_820_361_553,
        }),
      ).payload,
    ).toEqual({ symbol: 'PERP_ETH_USDT', price: EXTREME_SMALL_DECIMAL, timestamp: 1_618_820_361_553 });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('estfundingrate@PERP_BTC_USDT', {
          s: 'PERP_BTC_USDT',
          r: '-0.0001',
          ft: 1_618_820_360_000,
          ts: 1_618_820_200_000,
        }),
      ).payload,
    ).toMatchObject({ rate: '-0.0001', nextFundingTimestamp: 1_618_820_360_000 });
    expect(
      decodeWooV3WsDataFrame(
        publicFrame('openinterest@PERP_BTC_USDT', { s: 'PERP_BTC_USDT', oi: '936.37', ts: 1_618_820_200_000 }),
      ).payload,
    ).toMatchObject({ openInterest: '936.37' });
  });

  it('rejects public identity, sequence, decimal, tuple, and interval drift', () => {
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('bbo@PERP_BTC_USDT', {
          s: 'PERP_ETH_USDT',
          ap: '1',
          aq: '1',
          bp: '1',
          bq: '1',
          ts: 1,
        }),
      ),
    ).toThrow('does not match');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('orderbookupdate@PERP_BTC_USDT@50', {
          s: 'PERP_BTC_USDT',
          prevTs: 2,
          asks: [['1', 2]],
          bids: [],
          ts: 1,
        }),
      ),
    ).toThrow();
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('kline@PERP_BTC_USDT@1m', {
          s: 'PERP_BTC_USDT',
          t: '5m',
          o: '1',
          c: '1',
          h: '1',
          l: '1',
          v: '1',
          a: '1',
          st: 1,
          et: 2,
          ts: 1,
          tts: 1,
        }),
      ),
    ).toThrow('does not match');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('bbo@PERP_BTC_USDT', {
          s: 'PERP_BTC_USDT',
          ap: '-1',
          aq: '1',
          bp: '1',
          bq: '1',
          ts: 1,
        }),
      ),
    ).toThrow('positive decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('trade@PERP_BTC_USDT', {
          s: 'PERP_BTC_USDT',
          px: '1',
          sx: '-1',
          sd: 'BUY',
          src: 1,
          rpi: false,
          ts: 1,
        }),
      ),
    ).toThrow('positive decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('markprice@PERP_BTC_USDT', { s: 'PERP_BTC_USDT', px: '-1', ts: 1 }),
      ),
    ).toThrow('positive decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('openinterest@PERP_BTC_USDT', { s: 'PERP_BTC_USDT', oi: '-1', ts: 1 }),
      ),
    ).toThrow('non-negative decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('ticker@PERP_BTC_USDT', {
          s: 'PERP_BTC_USDT',
          o: '1',
          c: '1',
          h: '1',
          l: '1',
          v: '-1',
          a: '1',
          q: '1',
          u: '1',
          cnt: 1,
          ts: 1,
          tts: 1,
        }),
      ),
    ).toThrow('non-negative decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('kline@PERP_BTC_USDT@1m', {
          s: 'PERP_BTC_USDT',
          t: '1m',
          o: '1',
          c: '1',
          h: '1',
          l: '1',
          v: '-1',
          a: '1',
          st: 1,
          et: 2,
          ts: 1,
          tts: 1,
        }),
      ),
    ).toThrow('non-negative decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('orderbookupdate@PERP_BTC_USDT@50', {
          s: 'PERP_BTC_USDT',
          prevTs: 1,
          asks: [['1', '-1']],
          bids: [],
          ts: 2,
        }),
      ),
    ).toThrow('non-negative decimal');
    expect(() =>
      decodeWooV3WsDataFrame(
        publicFrame('orderbook10@PERP_BTC_USDT', {
          s: 'PERP_BTC_USDT',
          asks: [['1', '0']],
          bids: [],
          ts: 2,
        }),
      ),
    ).toThrow('positive decimal');
  });

  it('strictly decodes the three account-mode variants with an explicit replacement blocker', () => {
    const spot = decodeWooV3WsDataFrame(privateFrame('account', { m: 'PURE_SPOT', v: '10000' }));
    expect(spot.payload).toEqual({ mode: 'PURE_SPOT', totalAccountValue: '10000' });
    const margin = decodeWooV3WsDataFrame(
      privateFrame('account', {
        m: 'MARGIN',
        l: 4,
        mr: '7739.3757',
        omr: '7739.3757',
        imr: '1.0006',
        mmr: '0.0126',
        tc: '1146085.27376211',
        fc: '1145937.09993548',
        v: '10000',
      }),
    );
    expect(margin.payload).toMatchObject({ mode: 'MARGIN', leverage: 4 });
    const futures = decodeWooV3WsDataFrame(
      privateFrame('account', {
        m: 'FUTURE',
        M: 'ISOLATED',
        P: 'ONE_WAY',
        mr: '7739.3757',
        omr: '7739.3757',
        imr: '1.0006',
        mmr: '0.0126',
        tc: '1146085.27376211',
        fc: '1145937.09993548',
        v: '10000',
      }),
    );
    expect(futures.payload).toMatchObject({ mode: 'FUTURE', defaultMarginMode: 'ISOLATED', positionMode: 'ONE_WAY' });
    expect(futures.blockers).toEqual([
      WOO_V3_WS_BLOCKERS['private-account-replacement-semantics-undocumented'],
    ]);
  });

  it('strictly decodes private balance and position arrays without claiming replacement semantics', () => {
    const balance = decodeWooV3WsDataFrame(
      privateFrame('balance', {
        balances: [
          {
            t: 'BTC', h: '100', f: '0', i: '0', psq: '0', plq: '0', s: '0', u: '0', v: '50.5',
            l: '0', e: '0', aop: '0', pnl: '0', roi: '0.01', fee: '0', mp: '50000', ver: 0,
            b: '166', lcr: 30, ts: 1_618_757_713_353,
          },
        ],
      }),
    );
    expect(balance.payload).toEqual([
      expect.objectContaining({ token: 'BTC', holding: '100', trialFundBonus: '166', lossCoverRatio: 30 }),
    ]);
    expect(balance.blockers[0]?.code).toBe('private-balance-replacement-semantics-undocumented');

    const position = decodeWooV3WsDataFrame(
      privateFrame('position', {
        positions: [
          {
            s: 'PERP_BTC_USDT', h: '1', plq: '0.0002', psq: '0.0', aop: '0.0', pnl: '-1.55902',
            roi: '-0.0054', fee: '0.218', sp: '49000', mp: '50000', ot: 1_677_814_653_001,
            aq: 1, lv: 10, m: 'ISOLATED', ps: 'BOTH', it: 'USDT', ia: '1000', il: '10', is: '10',
            ver: 93454, ts: 1_677_814_653_001,
          },
        ],
      }),
    );
    expect(position.payload).toEqual([
      expect.objectContaining({ symbol: 'PERP_BTC_USDT', leverage: 10, marginMode: 'ISOLATED', positionSide: 'BOTH' }),
    ]);
    expect(position.blockers[0]?.code).toBe('private-position-replacement-semantics-undocumented');
  });

  it('decodes documented regular execution variants but blocks lifecycle authority', () => {
    const report = decodeWooV3WsDataFrame(
      privateFrame('executionreport', {
        mt: 0, s: 'PERP_BTC_USDT', cid: 0, oid: 54774393, t: 'MARKET', sd: 'BUY', ps: 'BOTH',
        sx: '1', px: '50000', tid: 56201985, esx: '1', epx: '50000', f: '0.0001', fa: 'USDT',
        tesx: '1', aepx: '50000', ss: 'FILLED', rs: '', tg: 'default', tf: '0.0001', tfc: 'USDT',
        tr: '0', trc: 'USDT', vsx: '1', ts: 1_675_406_261_689, ro: false, mk: false, lv: 10,
        rpi: false, m: 'CROSS',
      }),
    );
    expect(report.payload).toMatchObject({ messageType: 0, orderId: 54774393, status: 'FILLED' });
    expect(report.blockers[0]?.code).toBe('execution-status-lifecycle-incomplete');
    expect(
      decodeWooV3WsDataFrame(
        privateFrame('executionreport', { mt: 2, s: 'PERP_BTC_USDT', oid: 54774393, cid: 0, rs: 'reject' }),
      ).payload,
    ).toEqual({ messageType: 2, symbol: 'PERP_BTC_USDT', orderId: 54774393, clientOrderId: 0, reason: 'reject' });
    expect(
      decodeWooV3WsDataFrame(privateFrame('executionreport', { mt: 3, s: 'PERP_BTC_USDT', rs: 'reject' })).payload,
    ).toEqual({ messageType: 3, symbol: 'PERP_BTC_USDT', reason: 'reject' });
    expect(() => decodeWooV3WsDataFrame(privateFrame('executionreport', { mt: 4 }))).toThrow('0, 1, 2, or 3');
  });

  it('decodes only the documented algo row shape and keeps a typed shape/lifecycle blocker', () => {
    const frame = decodeWooV3WsDataFrame(
      privateFrame('algoexecutionreport', [
        {
          s: 'PERP_BAL_USDT', raid: 345181, paid: 0, aid: 345181, caid: 0, tg: 'default', ss: 'NEW',
          at: 'BRACKET', sd: 'SELL', ps: 'SHORT', sx: '1', trp: '100', trpt: 'MARK_PRICE', tss: 'SUCCESS',
          px: '50', t: 'LIMIT', tpx: '51', tt: 0, tid: 0, epx: '50', esx: '1', f: '0', fa: 'USDT',
          rs: '', tesx: '1', aepx: '50', tf: '0', tfc: 'USDT', tr: '0', trc: 'BAL', vsx: '1',
          ts: 1_667_978_011_834, ro: false, apx: '51', cr: '0.01', cv: '1', ep: '100', it: false,
          ia: false, im: false, ras: 'NEW', as: 'NEW', lv: 10, m: 'CROSS',
        },
      ]),
    );
    expect(frame.payload).toEqual([
      expect.objectContaining({ algoOrderId: 345181, algoType: 'BRACKET', triggerPriceType: 'MARK_PRICE' }),
    ]);
    expect(frame.blockers).toEqual([WOO_V3_WS_BLOCKERS['algo-shape-and-lifecycle-incomplete']]);
    expect(() =>
      decodeWooV3WsDataFrame(
        privateFrame('algoexecutionreport', [
          {
            s: 'PERP_BAL_USDT',
            raid: 345181,
          },
        ]),
      ),
    ).toThrow();
  });

  it('keeps all eight gates false and proves no active Woo runtime imports or transports this layer', () => {
    expect(Object.values(WOO_V3_MIGRATION_GATES)).toEqual([false, false, false, false, false, false, false, false]);
    expect(WOO_V3_WS_BLOCKERS['listen-key-rotation-overlap-undocumented']).toMatchObject({
      requires: 'credentialed-demo',
    });
    const wooDirectory = join(__dirname, '../../exchanges/woo');
    for (const runtimeFile of ['woo.api.ts', 'woo.exchange.ts', 'woo.ws-private.ts', 'woo.ws-public.ts']) {
      expect(readFileSync(join(wooDirectory, runtimeFile), 'utf8')).not.toContain('woo.v3.websocket');
    }
    const source = readFileSync(join(wooDirectory, 'woo.v3.websocket.ts'), 'utf8');
    expect(source).not.toMatch(/from ['"](?:axios|ws|@tealstreet\/reconnecting-websocket)['"]/);
    expect(source).not.toMatch(/\bnew WebSocket\b|\bfetch\s*\(/);
  });
});
