import type { Signature, SignerInterface, TypedData } from 'starknet';
import * as Starknet from 'starknet';
import { AccountSupport } from './starknet-account-support.js';
export type { SignerInterface as Signer, TypedData, Signature };
export declare function buildStarknetStarkKeyTypedData(starknetChainId: string): TypedData;
type StarknetKeypair = [string, string];
/**
 * This function borrows from starkware-crypto-utils's implementation
 * of `getPrivateKeyFromEthSignature()` where a deterministic
 * signature R segment, is hex encoded as the `keySeed` for
 * `grindKey()` along the Stark curve.
 */
export declare function getStarkKeypairFromStarknetSignature(signatureR: string): Promise<StarknetKeypair>;
export declare function getAccountSupport(account: Starknet.AccountInterface, starknetProvider: Starknet.ProviderInterface): Promise<AccountSupport>;
export declare function getPublicProvider(chainId: string): Starknet.ProviderInterface;
