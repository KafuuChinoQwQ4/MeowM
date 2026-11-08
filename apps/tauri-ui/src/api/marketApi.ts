import { invoke } from './client';
import { Channel } from '@tauri-apps/api/core';

export type MarketJson =
  null | boolean | number | string | MarketJson[] | { [key: string]: MarketJson };

export interface MarketApiParameter {
  name: string;
  type: string;
  required: boolean;
  location: string;
  description: string;
  example?: MarketJson;
}

export interface MarketApiEndpoint {
  id: string;
  product: string;
  category: string;
  title: string;
  method: string;
  path: string;
  host: string;
  transport: 'rest' | 'stream' | 'ws_api';
  auth: string;
  mutating: boolean;
  params: MarketApiParameter[];
  sample?: { [key: string]: MarketJson };
  doc_url: string;
  environments?: string[];
  supported?: boolean;
  unsupported_reason?: string | null;
}

export interface MarketApiRequest {
  endpoint_id: string;
  params: { [key: string]: MarketJson };
  environment?: string;
  credentials?: 'read' | 'trade' | 'withdraw';
  confirmed: boolean;
  request_id: string;
}

export interface MarketApiResponse {
  status: 'success' | 'failed' | 'unknown';
  code: string | null;
  message: string | null;
  data: MarketJson;
  request_id: string;
  http_status: number | null;
}

export interface BinanceStreamEvent {
  kind: string;
  sequence: number;
  data: MarketJson;
}

export interface BinanceStreamStopResult {
  status: 'closed' | 'unknown' | 'complete';
  request_id: string;
}

export function marketRequestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return [...bytes].map((value) => value.toString(16).padStart(2, '0')).join('');
}

export const marketApi = {
  binanceCatalogue: () => invoke<MarketApiEndpoint[]>('binance_api_catalogue'),
  ifindCatalogue: () => invoke<MarketApiEndpoint[]>('ifind_api_catalogue'),
  binanceRequest: (request: MarketApiRequest) =>
    invoke<MarketApiResponse>('binance_api_request', { request }),
  binanceSocketRequest: (request: MarketApiRequest) =>
    invoke<MarketApiResponse>('binance_socket_request', { request }),
  startBinanceStream: (request: MarketApiRequest, handler: (event: BinanceStreamEvent) => void) => {
    const onEvent = new Channel<BinanceStreamEvent>(handler);
    return invoke<string>('binance_ws_start', { request, onEvent });
  },
  stopBinanceStream: (sessionId: string) =>
    invoke<BinanceStreamStopResult>('binance_ws_stop', { sessionId }),
  ackBinanceStream: (sessionId: string, sequence: number) =>
    invoke<void>('binance_ws_ack', { sessionId, sequence }),
  ifindRequest: (request: MarketApiRequest) =>
    invoke<MarketApiResponse>('ifind_api_request', { request }),
};
