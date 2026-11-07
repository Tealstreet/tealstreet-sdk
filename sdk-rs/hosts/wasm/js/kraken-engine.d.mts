/** Kraken-family WASM module only. Other family modules do not export these members. */
export * from './engine.mjs';
export {default} from './engine.mjs';

/** Kraken-family pure codec exports; absence is not capability negotiation. */
export function krakenHostedDescriptor(workJson: string): string;
export function krakenHostedValidate(descriptorJson: string, receiptJson: string, slot: string, roleJson: string, now: number, deadline: number): void;
export function krakenHostedVerify(descriptorJson: string, resultJson: string, publicCredential: string): void;
/** Trusted signing-only installation. Never pass setup through the op channel. */
export class HostedOperationExecutor {
  constructor(setupJson: string, spot: boolean, testnet: boolean);
  binding(slot: string): string;
  sign(descriptorJson: string, receiptJson: string, slot: string, roleJson: string, now: number, deadline: number): string;
  free(): void;
}
