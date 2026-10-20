// A failed/aborted capture may be useful for diagnosis but cannot prove a
// completed lifecycle, even when TS and Rust replay its errors identically.
export function assertLiveSmokeComplete(records) {
  if (!records.some((row) => row.type === 'meta' && row.liveSmoke === true)) return;
  const results = records.filter((row) => row.type === 'live-smoke-result');
  const result = results[0];
  if (results.length !== 1 || result.ok !== true || result.cleanupComplete !== true || result.authoritative !== true || result.stage !== 'complete' ||
      typeof result.symbol !== 'string' || !result.symbol)
    throw new Error('Live-smoke tape is incomplete or failed; it cannot pass the lifecycle gate');
  const verified = records.filter((row) => row.type === 'live-smoke-verification');
  if (verified.length !== 1 || verified[0].ok !== true) throw new Error('Live-smoke tape has no successful authoritative readback');
  const commands = records.filter((row) => row.type === 'command');
  const required = ['placeOrders', 'amendOrder', 'cancelOrders', 'cancelSymbolOrders', 'setLeverage'];
  let index = 0;
  for (const row of commands) if (row.op === required[index]) index++;
  if (index !== required.length || JSON.stringify(result.commands) !== JSON.stringify(commands.map((row) => row.op)))
    throw new Error('Live-smoke tape is missing lifecycle commands');
  return result;
}
