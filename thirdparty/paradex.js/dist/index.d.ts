import * as _Account from './account.js';
import * as _Config from './config.js';
import * as _Signer from './ethereum-signer.js';
import * as _ParaclearProvider from './paraclear-provider.js';
import * as _Paraclear from './paraclear.js';
export declare const Config: {
    fetchConfig: typeof _Config.fetchConfig;
};
export declare const Account: {
    fromEthSigner: typeof _Account.fromEthSigner;
    fromStarknetAccount: typeof _Account.fromStarknetAccount;
};
export declare const Signer: {
    ethersSignerAdapter: typeof _Signer.ethersSignerAdapter;
};
export declare const ParaclearProvider: {
    DefaultProvider: typeof _ParaclearProvider.DefaultProvider;
};
export declare const Paraclear: {
    getTokenBalance: typeof _Paraclear.getTokenBalance;
    getSocializedLossFactor: typeof _Paraclear.getSocializedLossFactor;
    getReceivableAmount: typeof _Paraclear.getReceivableAmount;
    withdraw: typeof _Paraclear.withdraw;
};
