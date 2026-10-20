import { loadBundledRustEngine } from '../src/rust-engine';

const coinbase = await loadBundledRustEngine('coinbasespot');
if ((await loadBundledRustEngine('coinbase')) !== coinbase) throw new Error('Coinbase aliases loaded separate modules');
const binance = await loadBundledRustEngine('binance');
if (binance === coinbase || (await loadBundledRustEngine('binancepm')) !== binance)
  throw new Error('Embedded engine families are not isolated');
let rejected = false;
try {
  new binance.WasmEngine('coinbase', '{}');
} catch {
  rejected = true;
}
if (!rejected) throw new Error('Binance artifact included another engine family');
console.log('rust-smoke: isolated Binance and Coinbase families initialized');

// The fixture owns documented Coinbase public inputs; the registered CLI loader
// remains authoritative because the optional sibling-file MCP loader is absent.
await import('../../../packages/exchange-mcp/scripts/rust-probe');
