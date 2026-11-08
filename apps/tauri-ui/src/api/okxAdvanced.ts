import { Channel } from '@tauri-apps/api/core';
import { invoke } from './client';

export type OkxEnvironment = 'demo' | 'production';
export type OkxCredentialScope = 'read' | 'trade' | 'withdraw';
export type Json = null | boolean | number | string | Json[] | { [key: string]: Json };

// JSON.parse converts numbers to binary floating point before IPC. Reject values
// that cannot be represented exactly; OKX's decimal fields are strings.
export function assertSafeOkxJsonNumbers(value: Json): void {
  if (typeof value === 'number' && !Number.isSafeInteger(value)) {
    throw new Error('JSON 中大整数或小数请用引号包围，避免精度丢失。');
  }
  if (Array.isArray(value)) value.forEach(assertSafeOkxJsonNumbers);
  else if (value && typeof value === 'object')
    Object.values(value).forEach(assertSafeOkxJsonNumbers);
}

export interface OkxApiParameter {
  name: string;
  type: string;
  required: boolean;
  conditional: boolean;
  description: string;
}

export interface OkxRestEndpoint {
  id: string;
  category: string;
  group: string;
  title: string;
  method: string;
  path: string;
  auth: 'public' | 'read' | 'trade' | 'withdraw';
  mutating: boolean;
  body_kind: 'object' | 'array';
  parameters: OkxApiParameter[];
  doc_url: string;
  rate_limit: string;
  already_integrated: boolean;
  supports_exp_time?: boolean;
}

export interface OkxWsEndpoint {
  id: string;
  category: string;
  group: string;
  title: string;
  service: 'public' | 'private' | 'business' | 'public-sbe';
  requires_login: boolean;
  operation: string;
  channels: string[];
  parameters: OkxApiParameter[];
  sample_args: { [key: string]: Json }[];
  doc_url: string;
  mutating: boolean;
}

export interface OkxApiCatalogue {
  source_url: string;
  retrieved_on: string;
  rest: OkxRestEndpoint[];
  websocket: OkxWsEndpoint[];
}

export interface OkxApiRequest {
  endpoint_id: string;
  environment: OkxEnvironment;
  credentials: OkxCredentialScope;
  params: { [key: string]: Json } | { [key: string]: Json }[];
  confirmed: boolean;
  request_id: string;
  exp_time: string | null;
}

export interface OkxApiResponse {
  status: 'success' | 'rejected' | 'partial_failure' | 'unknown';
  code: string | null;
  message: string | null;
  data: Json;
  request_id: string;
  http_status: number | null;
}

export interface OkxStreamRequest {
  endpoint_id: string;
  environment: OkxEnvironment;
  credentials: OkxCredentialScope;
  args: { [key: string]: Json }[];
  request_id: string;
  confirmed: boolean;
  exp_time: string | null;
}

export interface OkxStreamEvent {
  kind: 'status' | 'message' | 'error' | 'closed';
  data: Json;
  sequence?: number;
}

export interface OkxStreamStopResult {
  status: 'closed' | 'unknown' | 'complete';
  request_id: string;
}

export function okxRequestId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  return [...bytes].map((value) => value.toString(16).padStart(2, '0')).join('');
}

export const okxAdvancedApi = {
  catalogue: () => invoke<OkxApiCatalogue>('okx_api_catalogue'),
  request: (request: OkxApiRequest) => invoke<OkxApiResponse>('okx_api_request', { request }),
  startStream: (request: OkxStreamRequest, handler: (event: OkxStreamEvent) => void) => {
    const onEvent = new Channel<OkxStreamEvent>(handler);
    return invoke<string>('okx_ws_start', { request, onEvent });
  },
  stopStream: (sessionId: string) => invoke<OkxStreamStopResult>('okx_ws_stop', { sessionId }),
  ackStream: (sessionId: string, sequence: number) =>
    invoke<void>('okx_ws_ack', { sessionId, sequence }),
};
