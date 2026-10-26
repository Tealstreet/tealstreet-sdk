import { createRustRegistry } from './rustRegistry';

declare const require: <T = any>(id: string) => T;

export const exchangeRegistry = createRustRegistry(async () => require('./rust/rust.exchange').RustEngineExchange);

export { getRegisteredExchangeNames } from './rustRegistry';
