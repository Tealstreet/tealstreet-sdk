import * as Starknet from 'starknet';
import type { ParadexConfig } from './config.js';
export declare class DefaultProvider extends Starknet.RpcProvider {
    constructor(config: ParadexConfig);
}
export type ParaclearProvider = DefaultProvider;
