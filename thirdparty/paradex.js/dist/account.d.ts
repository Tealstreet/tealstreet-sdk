import * as Starknet from 'starknet';
import type { ParadexConfig } from './config.js';
import * as ethereumSigner from './ethereum-signer.js';
export interface Account extends Starknet.Account {
}
interface FromEthSignerParams {
    readonly provider: Starknet.ProviderOptions | Starknet.ProviderInterface;
    readonly config: ParadexConfig;
    readonly signer: ethereumSigner.EthereumSigner;
}
/**
 * Generates a Paradex account from an Ethereum wallet.
 * @returns The generated Paradex account.
 */
export declare function fromEthSigner({ provider, config, signer, }: FromEthSignerParams): Promise<Account>;
interface FromStarknetAccountParams {
    /** Paradex chain provider */
    readonly provider: Starknet.ProviderOptions | Starknet.ProviderInterface;
    readonly config: ParadexConfig;
    readonly account: Starknet.AccountInterface;
    readonly starknetProvider?: Starknet.ProviderInterface;
}
/**
 * Generates a Paradex account from a Starknet signer.
 * @returns The generated Paradex account.
 */
export declare function fromStarknetAccount({ provider, config, account, starknetProvider, }: FromStarknetAccountParams): Promise<Account>;
/**
 * Checks if starknet signatures are equal.
 */
export declare function isStarknetSignatureEqual(signature: Starknet.Signature, additionalSignature: Starknet.Signature): boolean;
export {};
