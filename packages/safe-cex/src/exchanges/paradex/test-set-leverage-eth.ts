import { ParadexExchange } from './paradex.exchange';
import { DefaultStore } from '../../store/store.base';

const test = async () => {
  console.log('Testing Paradex setLeverage for ETH-USD-PERP...');

  const store = new DefaultStore();
  const exchange = new ParadexExchange('paradex', {
    key: process.env.PARADEX_API_KEY || '',
    secret: process.env.PARADEX_PRIVATE_KEY || '',
    testnet: false,
  }, store);

  try {
    console.log('Setting leverage to 28x for ETH-USD-PERP...');
    await exchange.setLeverage('ETH-USD-PERP', 28);
    console.log('✓ Leverage set successfully');
  } catch (error: any) {
    console.error('✗ Failed to set leverage');
    console.error('Error message:', error.message);
    console.error('Full error:', error);
    if (error.response) {
      console.error('Response data:', JSON.stringify(error.response.data, null, 2));
      console.error('Response status:', error.response.status);
    }
  } finally {
    exchange.dispose();
  }
};

test();
