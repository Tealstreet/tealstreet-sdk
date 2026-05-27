import { ethers } from 'ethers';

export const createWalletFromPrivateKey = (privateKey: string) => {
  return new ethers.Wallet(privateKey);
};
