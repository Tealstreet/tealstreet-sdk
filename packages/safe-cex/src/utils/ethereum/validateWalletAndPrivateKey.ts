import { ethers } from 'ethers';

export const validateWalletAndPrivateKey = (privateKey: string, address: string) => {
  const wallet = new ethers.Wallet(privateKey);
  return wallet.address?.toLowerCase() === address?.toLowerCase();
};
