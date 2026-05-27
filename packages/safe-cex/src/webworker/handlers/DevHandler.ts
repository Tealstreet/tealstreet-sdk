import { howWebSocketJA, howWebSocketSG } from '../../httpOverWebsocket/axiosHttpOverWebsocketAdapter';
import { StoreWorkerPayload } from '../StoreWorkerPayload';
import { BaseHandler } from './BaseHandler';

export class DevHandler extends BaseHandler {
  handle(payload: StoreWorkerPayload): void {
    switch (payload.op) {
      case 'dev:simulateWebSocketsReconnect':
        this._exchange.getAllSockets().forEach((socket) => socket?.reconnect());
        break;
      case 'dev:simulateHOWClose':
        howWebSocketJA.simulateClose();
        howWebSocketSG.simulateClose();
        break;
      case 'dev:forceOpen':
        this._exchange.getAllSockets().forEach((socket) => socket?.forceOpen());
        break;
      case 'dev:forceClose':
        this._exchange.getAllSockets().forEach((socket) => socket?.forceClose());
        break;
    }
  }
}
