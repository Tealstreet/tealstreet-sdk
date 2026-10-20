// Expected conditional values come from the recorded venue, never the Rust output.
const parse = (text) => JSON.parse(text, (_key, value, context) => {
  if (typeof value === 'number' && !Number.isSafeInteger(value) && Number.isInteger(value)) {
    if (!context?.source) throw new Error('lossless JSON number source unavailable');
    if (/^-?\d+$/.test(context.source)) return context.source;
  }
  return value;
});
export function fixWeexConditionals(account, records) {
  const expected = new Map();
  const observe = (row) => {
    const id = row.algoId ?? row.orderId ?? row.id;
    if (id == null) return;
    const type = row.orderType ?? row.type;
    const trigger = Number(row.triggerPrice);
    const protective = [true, 'true', '1', 1].includes(row.reduceOnly)
      || [true, 'true', '1', 1].includes(row.closePosition)
      || [true, 'true', '1', 1].includes(row.positionTpsl)
      || ['TPSL', 'TP_SL'].includes(row.algoType)
      || ['TAKE_PROFIT', 'STOP_LOSS'].includes(row.planType);
    if (!protective && ['STOP', 'STOP_MARKET', 'TAKE_PROFIT', 'TAKE_PROFIT_MARKET'].includes(type)
      && Number.isFinite(trigger) && trigger > 0) expected.set(String(id), trigger);
    else expected.delete(String(id));
  };
  for (const record of [...records].sort((a, b) => (a.effectiveAt ?? a.t) - (b.effectiveAt ?? b.t))) {
    if (record.type === 'http' && record.status >= 200 && record.status < 300) {
      if (record.method === 'GET' && record.url.includes('/openAlgoOrders')) {
        const rows = parse(record.body);
        if (Array.isArray(rows)) rows.forEach(observe);
      } else if (record.method === 'POST' && new URL(record.url).pathname.endsWith('/algoOrder') && record.requestBody) {
        observe({ ...parse(record.requestBody), ...parse(record.body) });
      }
    } else if (record.type === 'ws') {
      const message = parse(record.text);
      if (message.e === 'orders' && Array.isArray(message.d)) message.d.forEach(observe);
    }
  }
  for (const order of account.orders) {
    if (!order.reduceOnly && expected.has(order.id)
      && ['conditional', 'stop_market', 'take_profit_market'].includes(order.type)) {
      order.type = 'conditional';
      order.price = expected.get(order.id);
    }
  }
}

// BlofinMetadata.supportsPositionMargin is false, and its TS mapPositions omits
// marginSize. Store merges can retain a zero-size placeholder's marginSize=0;
// the unavailable field is null in the comparison, independently of Rust.
export function fixBlofinAccount(account) {
  for (const position of account.positions) {
    if (position.marginSize === 0) position.marginSize = null;
  }
  if (account.positions.length === 0) {
    account.balance.used = 0;
    account.balance.upnl = 0;
  }
}

/** Phemex's execution, cumulative-size and flat-settings oracle uses venue rows only. */
export function fixPhemexAccount(account, tapeRecords) {
    // TS deduplicates executions by order within each message, then repeats
    // executions echoed by later messages. Build the oracle only from the tape.
    const seen = new Set();
    account.fills = [];
    for (const record of tapeRecords) {
      if (record.type !== 'ws') continue;
      const message = JSON.parse(record.text);
      if (message.type === 'snapshot') continue;
      for (const row of message.orders_p ?? []) {
        if (!['MakerFill', 'TakerFill'].includes(row.execStatus) || !row.execID || seen.has(row.execID)) continue;
        seen.add(row.execID);
        account.fills.push({ id: row.orderID, symbol: row.symbol.toUpperCase(), side: row.side.toLowerCase(), amount: Number(row.execQty), price: Number(row.execPriceRp) });
      }
    }
    const cumulative = new Map();
    for (const record of tapeRecords) {
      if (record.type !== 'ws') continue;
      for (const row of JSON.parse(record.text).orders_p ?? []) {
        const filled = Number(row.cumQty ?? row.cumQtyRq);
        const amount = Number(row.orderQty ?? row.orderQtyRq);
        if (Number.isFinite(filled) && Number.isFinite(amount)) cumulative.set(row.orderID, { filled, remaining: Math.max(0, amount - filled) });
      }
    }
    for (const order of account.orders) {
      const fixed = cumulative.get(order.id);
      if (fixed) Object.assign(order, fixed);
    }
    // TS uses last execution size, then zeroes canceled fills; cumulative venue
    // quantities above retain exposure independently of Rust's mapped rows.
    // Other native safety regressions: failed slices retain prior currency;
    // final events fence stale snapshots, Triggered remains non-terminal;
    // ambiguous writes never retry; split units preserve exact requested size;
    // partial-fill cancel/replace refuses uncertain remaining quantity;
    // zero-open tickers report zero percentage, and a full WS snapshot replaces
    // old depth. TS public dispatch now preserves outer sequence for buffering,
    // explicitly requests depth0 (default30 leaves deep levels stale), and
    // replaces periodic full snapshots.

  const settings = new Map(account.settings.map((row) => [row.symbol, row]));
  const observed = new Map();
  const observe = (row, fallback) => {
    const leverage = Math.abs(Number(row.leverageRr));
    if (!leverage || !Number.isFinite(leverage) || !row.symbol) return;
    const symbol = row.symbol.toUpperCase(), side = row.posSide ?? 'Merged';
    const raw = row.transactTimeNs ?? fallback ?? 0;
    const version = /^\d+$/.test(String(raw)) ? BigInt(raw) : 0n;
    const key = symbol + ':' + side;
    if ((observed.get(key)?.version ?? -1n) > version) return;
    observed.set(key, { symbol, side, version, leverage, marginType: Number(row.leverageRr) < 0 || row.crossMargin ? 'cross' : 'isolated' });
  };
  // TS drops zero-sized rows. Use the latest authoritative row per side,
  // preferring long/merged leverage as the single market setting does.
  for (const record of [...tapeRecords].sort((a,b) => (a.effectiveAt ?? a.availableAfter ?? a.t ?? 0) - (b.effectiveAt ?? b.availableAfter ?? b.t ?? 0))) {
    if (record.type === 'http' && record.status === 200 && new URL(record.url).pathname === '/g-accounts/accountPositions') {
      for (const row of parse(record.body).data?.positions ?? []) observe(row);
    } else if (record.type === 'ws') {
      const message = parse(record.text);
      for (const row of message.positions_p ?? []) observe(row, message.timestamp);
    }
  }
  const missing = new Map();
  for (const row of observed.values()) {
    if (settings.has(row.symbol)) continue;
    const previous = missing.get(row.symbol);
    if (!previous || previous.side === 'Short' || row.side !== 'Short') missing.set(row.symbol, row);
  }
  for (const {symbol,leverage,marginType} of missing.values()) settings.set(symbol,{symbol,leverage,marginType});
  account.settings = [...settings.values()].sort((a,b) => a.symbol.localeCompare(b.symbol));
}

// TS emits the FILLED order total and the execution for one Lighter trade.
// Build its corrected fill oracle from recorded execution IDs and quantities.
export function fixLighterFills(account, records) {
  const markets = new Map();
  const marketIds = new Map();
  for (const record of records) {
    if (record.type !== 'http' || !/\/orderBook(?:Details|s)$/.test(new URL(record.url).pathname) || record.status !== 200) continue;
    const payload = parse(record.body);
    for (const market of payload.order_book_details ?? payload.order_books ?? []) {
      markets.set(String(market.market_id), market.symbol);
      marketIds.set(market.symbol, String(market.market_id));
    }
  }
  const flatten = (value, market) => Array.isArray(value) ? value.flatMap((row) => flatten(row, market)) : value && typeof value === 'object'
    ? ('price' in value && ('size' in value || 'amount' in value)) ? [{fill:value,market}]
      : Object.entries(value).flatMap(([key,row]) => flatten(row,key)) : [];
  const fills = new Map();
  const seen = new Set();
  let observed = false;
  const accountId = records.find((record) => record.type === 'config')?.options?.applicationId;
  for (const record of records) {
    if (record.type !== 'ws') continue;
    const message = parse(record.text);
    const kind = String(message.type ?? '');
    if (!String(message.channel ?? '').startsWith('account_all') || !/^(update|subscribed)/.test(kind)) continue;
    const payload = message.data ?? message;
    if (payload.fills === undefined && payload.trades === undefined) continue;
    observed = true;
    for (const {fill,market:hint} of [payload.fills,payload.trades].flatMap((source) => flatten(source))) {
      const execution = fill.trade_id_str ?? fill.tradeId ?? fill.trade_id ?? fill.fillId ?? fill.fill_id;
      const market = String(fill.market_id ?? fill.marketId ?? hint ?? '');
      const symbol = fill.symbol || markets.get(market);
      const side = String(fill.ask_account_id ?? '') === String(accountId) ? 'sell'
        : String(fill.bid_account_id ?? '') === String(accountId) ? 'buy' : String(fill.side ?? '').toLowerCase();
      const id = fill.orderId ?? fill.order_id ?? (side === 'sell' ? fill.ask_id : fill.bid_id);
      const amount = Number(fill.size ?? fill.amount), price = Number(fill.price);
      if (execution == null || String(execution) === '' || !symbol || !['buy','sell'].includes(side) || id == null || !(amount > 0) || !(price > 0)) continue;
      const key = `${marketIds.get(symbol) ?? (market || symbol)}:${execution}`;
      if (seen.has(key)) continue;
      seen.add(key);
      if (kind.startsWith('subscribed')) continue;
      fills.set(key, {id:String(id),symbol,side,amount,price});
    }
  }
  if (observed) account.fills = [...fills.values()];
}

export function fixLighterCommands(results, records) {
  const commands=records.filter(record=>record.type==='command');
  for(let index=0;index<commands.length;index++) {
    const command=commands[index];
    // The venue is net only. A request to retain that mode needs no write;
    // TS inherits the base unsupported method instead of acknowledging it.
    if(command.op==='setPositionMode' && command.hedged===false && results[index]?.error==='Not implemented') results[index]={ok:null};
  }
}

// Kraken TS truncates full depth to 100 and emits repeated execution IDs.
// Reconstruct those two expected outputs from the recording, never Rust state.
export function fixKrakenOutput(ts,records){
 const books=new Map(Object.keys(ts.books).map(s=>[s,{seq:undefined,bids:new Map(),asks:new Map()}]));
 const markets=new Map(ts.markets.map(m=>[m.symbol,m]));
 const tickers=new Map((ts.tickers??[]).map(t=>[t.symbol,t]));
 const fills=[],seen=new Set();let coins;
 const extraReads=new Set();
 const reads=records.filter(r=>r.type==='http'&&r.method==='GET'&&r.status===200);
 const redundant=new Set();
 // Require command-owned anchors; a short page in another query cannot excuse
 // a missing Rust page. An ambiguous same-stage full page disables correction.
 for(const source of reads){
  if(source.afterCommand===undefined)continue;
  const u=new URL(source.url),data=JSON.parse(source.body);
  const sameStage=reads.filter(r=>r.afterCommand===source.afterCommand&&new URL(r.url).pathname===u.pathname);
  if(u.pathname==='/derivatives/api/v3/fills'&&!u.search&&data.fills?.length>0&&data.fills.length<100&&!sameStage.some(r=>JSON.parse(r.body).fills?.length>=100)){
   const oldest=Math.min(...data.fills.map(f=>Date.parse(f.fillTime)));
   const cursor=new Date(Math.floor(oldest/1000)*1000+1).toISOString();
   for(const page of sameStage){const p=new URL(page.url);if(p.searchParams.size===1&&p.searchParams.get('lastFillTime')===cursor&&JSON.parse(page.body).fills?.length===0)redundant.add(page);}
  }
  if(u.pathname==='/api/history/v3/account-log'&&data.logs?.length>0&&data.logs.length<500&&!sameStage.some(r=>JSON.parse(r.body).logs?.length>=500)){
   const cash=data.logs.filter(r=>r.asset?.toLowerCase()!==r.contract?.toLowerCase());
   if(!cash.length)continue;
   const boundary=Math.max(...cash.map(r=>Date.parse(r.date)));
   for(const page of sameStage){const p=new URL(page.url);if(p.searchParams.size===2&&p.searchParams.get('since')===String(boundary)&&p.searchParams.get('count')===u.searchParams.get('count')&&JSON.parse(page.body).logs?.length===0)redundant.add(page);}
  }
 }
 for(const record of records){
  if(record.type==='http'&&record.method==='GET'&&record.status===200){
   const u=new URL(record.url),data=JSON.parse(record.body);
   if(u.pathname==='/derivatives/api/v3/instruments'&&data.result==='success')for(const row of data.instruments??[]){
    if(Number(row.contractValueTradePrecision)<0){const market=markets.get(row.symbol);if(market)market.precision.amount=10**(-Number(row.contractValueTradePrecision));}
   }
   // TS derives fill cursors after flooring to seconds, and its income pager
   // requests an extra boundary page even when the native page was short.
   if(redundant.has(record)&&!records.some(other=>other!==record&&other.type==='http'&&other.method==='GET'&&other.url===record.url&&!redundant.has(other))){
    const params=[...u.searchParams].sort(([a],[b])=>a.localeCompare(b)).map(([k,v])=>`${encodeURIComponent(k)}=${encodeURIComponent(v)}`).join('&');
    extraReads.add(`GET ${u.origin}${u.pathname}?${params}`);
   }
   if(u.pathname==='/derivatives/api/v3/accounts'&&data.result==='success'&&data.accounts){
    const holdings=new Map();
    for(const [name,account]of Object.entries(data.accounts)){
     if(['cash','cashAccount'].includes(name))for(const [coin,amount]of Object.entries(account.balances??{}))holdings.set(coin.toUpperCase(),[Number(amount),Number(amount)]);
    }
    const flex=data.accounts.flex??data.accounts.multiCollateralMarginAccount;
    if(flex?.type==='multiCollateralMarginAccount')for(const [coin,v]of Object.entries(flex.currencies??{}))holdings.set(coin.toUpperCase(),[Number(v.quantity??0),Number(v.value??0)]);
    for(const [name,a]of Object.entries(data.accounts))if(name.startsWith('fi_')&&a.auxiliary){const coin=(a.auxiliary.currency??'USD').toUpperCase(),v=Number(a.auxiliary.pv??0),prior=holdings.get(coin)??[0,0];holdings.set(coin,[prior[0]+v,prior[1]+v]);}
    coins=Object.fromEntries([...holdings].map(([coin,[amount,notional]])=>[coin,{amount,notional}]));
   }
  }
  if(record.type!=='ws')continue;
  const m=JSON.parse(record.text);
  if(m.feed==='ticker'&&!m.event){
   const t=tickers.get(m.product_id);
   if(t){
    // Full native tickers omit a zero relative rate; TS keeps its stale rate.
    t.fundingRate=Number(m.relative_funding_rate??0);
    for(const [native,field]of [['volume','volume'],['volumeQuote','quoteVolume'],['openInterest','openInterest'],['change','percentage']])if(m[native]!=null)t[field]=Number(m[native]);
    const open=Number(m.open_24h??m.open24h??m.open);
    if(m.change==null&&m.last!=null&&open>0)t.percentage=(Number(m.last)-open)/open*100;
   }
  }
  if(m.feed==='fills_snapshot'||m.feed==='fills')for(const f of m.fills??[]){
   if(!f.fill_id||seen.has(f.fill_id))continue;seen.add(f.fill_id);
   if(m.feed==='fills'&&markets.has(f.instrument))fills.push({id:String(f.order_id),symbol:f.instrument,side:f.buy?'buy':'sell',amount:Number(f.qty),price:Number(f.price)});
  }
  const book=books.get(m.product_id);if(!book||!['book','book_snapshot'].includes(m.feed)||m.event)continue;
  if(m.feed==='book_snapshot'){
   if(book.seq!==undefined&&m.seq<=book.seq)continue;
   if(!Array.isArray(m.bids)||!Array.isArray(m.asks))continue;
   book.bids=new Map(m.bids.map(l=>[Number(l.price),Number(l.qty)]));book.asks=new Map(m.asks.map(l=>[Number(l.price),Number(l.qty)]));book.seq=m.seq;
  }else{
   if(book.seq===undefined||m.seq<=book.seq)continue;
   if(m.seq!==book.seq+1){book.seq=undefined;book.bids.clear();book.asks.clear();continue;}
   const side=m.side==='buy'?book.bids:book.asks,price=Number(m.price),qty=Number(m.qty);if(qty===0)side.delete(price);else side.set(price,qty);book.seq=m.seq;
  }
 }
 for(const[s,b]of books){if(b.seq===undefined)continue;const scale=markets.get(s)?.contractSize??1;ts.books[s]={bids:[...b.bids].filter(([,q])=>q>0).sort(([a],[b])=>b-a).map(([p,q])=>[p,q*scale]),asks:[...b.asks].filter(([,q])=>q>0).sort(([a],[b])=>a-b).map(([p,q])=>[p,q*scale])};}
 if(ts.account){
  ts.account.fills=fills;if(coins!==undefined)ts.account.balance.coins=coins;
  // Kraken publishes no position margin allocation; TS's base store invents 0.
  for(const p of ts.account.positions)if(p.marginSize===0)p.marginSize=null;
 }
 ts.requests=ts.requests.filter(key=>!extraReads.has(key));
 ts.subscriptions=ts.subscriptions.filter(key=>!key.includes('"feed":"account_balances"'));
}
