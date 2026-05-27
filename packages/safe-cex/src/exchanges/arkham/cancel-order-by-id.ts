import { createHmac } from 'crypto';
import axios from 'axios';
import fs from 'fs';
import path from 'path';

const credentialsPath = path.join(__dirname, '.apikey.json');
const credentials = JSON.parse(fs.readFileSync(credentialsPath, 'utf-8'));

const API_KEY = credentials.apiKey;
const API_SECRET = credentials.secret;
const BASE_URL = 'https://arkm.com/api';
const ORDER_ID = 6193513383419;

function createSignature(
  apiKey: string,
  apiSecret: string,
  method: string,
  path: string,
  body: string = ''
): { signature: string; expiry: number } {
  const timestamp = Date.now() * 1000;
  const expiry = timestamp + 5 * 1000000;
  const message = `${apiKey}${expiry}${method}${path}${body}`;
  const decodedSecret = Buffer.from(apiSecret, 'base64');
  const hexSecret = decodedSecret.toString('hex');
  const signature = createHmac('sha256', Buffer.from(hexSecret, 'hex'))
    .update(message)
    .digest('base64');
  return { signature, expiry };
}

async function cancelOrder() {
  const endpoint = '/orders/cancel';
  const method = 'POST';
  const cancelData = { orderId: ORDER_ID };
  const body = JSON.stringify(cancelData);
  const { signature, expiry } = createSignature(API_KEY, API_SECRET, method, endpoint, body);

  try {
    const response = await axios.post(`${BASE_URL}${endpoint}`, cancelData, {
      headers: {
        'Arkham-Api-Key': API_KEY,
        'Arkham-Signature': signature,
        'Arkham-Expires': expiry.toString(),
        'Arkham-Broker-Id': '1002',
        'Content-Type': 'application/json',
        Accept: 'application/json',
      },
    });

    console.log('✅ Order cancelled!');
    console.log(JSON.stringify(response.data, null, 2));
  } catch (error: any) {
    console.log('❌ Failed!');
    console.log('Error:', error.response?.data || error.message);
  }
}

cancelOrder();
