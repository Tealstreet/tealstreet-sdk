import { readFileSync } from 'node:fs';
// Bun's Node compatibility module initializes its own codec exports from globals.
// Load that runtime shim before inspecting the SDK graph without those globals.
import 'node:util';

const decoder = globalThis.TextDecoder;
const encoder = globalThis.TextEncoder;
const wasm = globalThis.WebAssembly;
Reflect.deleteProperty(globalThis, 'TextDecoder');
Reflect.deleteProperty(globalThis, 'TextEncoder');
Reflect.deleteProperty(globalThis, 'WebAssembly');
const { setReactNativeRustEngineLoader } = await import('../src');
const { exchangeRegistry } = await import('@tealstreet/safe-cex/src/exchanges/registry.native');
const NativeExchange = await exchangeRegistry.binancespot();
const { RustEngineExchange, setRustEngineTransport } =
  await import('@tealstreet/safe-cex/src/exchanges/rust/rust.exchange');
if (NativeExchange !== RustEngineExchange || (await exchangeRegistry.binance()) !== RustEngineExchange)
  throw new Error('Native registry did not resolve the Rust Binance family');
// Registering the host bridge also needs neither codecs nor WebAssembly. Actual
// binary frame I/O below uses the runtime's restored codec/WASM implementation.
await setReactNativeRustEngineLoader(async () => {
  const module = await import('@tealstreet/safe-cex-wasm/web');
  await module.default({
    module_or_path: readFileSync(new URL('../../../sdk-rs/hosts/wasm/pkg/web/safe_cex_wasm_bg.wasm', import.meta.url)),
  });
  return module;
});
globalThis.TextDecoder = decoder;
globalThis.TextEncoder = encoder;
globalThis.WebAssembly = wasm;
const { DefaultStore } = await import('@tealstreet/safe-cex/src/store/store.base');
const { realVenue } = await import('@tealstreet/safe-cex/src/__tests__/cutover/realVenue');
const fixture = realVenue('binancespot');
setRustEngineTransport(() => fixture.transport);
const exchange = new NativeExchange('binancespot', fixture.options, new DefaultStore());
let timeout: ReturnType<typeof setTimeout> | undefined;
try {
  await Promise.race([
    exchange.start(),
    new Promise<never>((_resolve, reject) => {
      timeout = setTimeout(
        () => reject(new Error(`Native registry startup failed: ${fixture.errors.join('; ')}`)),
        15000,
      );
    }),
  ]);
  if (!exchange.store.loaded.markets || !exchange.store.markets.length) throw new Error('Native catalog is empty');
  if (fixture.errors.length) throw new Error(fixture.errors.join('; '));
  console.log('mobile-native-registry: Binance aliases loaded a native catalog');
} finally {
  clearTimeout(timeout);
  exchange.dispose();
  fixture.dispose();
}
