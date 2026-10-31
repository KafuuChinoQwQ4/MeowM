import { invoke } from './client';

export type ProviderId = 'wechat_pay' | 'alipay' | 'binance' | 'okx' | 'ifind';

export type CredentialInput =
  | {
      provider: 'wechat_pay';
      merchant_id: string;
      certificate_serial: string;
      private_key_pem: string;
    }
  | {
      provider: 'alipay';
      app_id: string;
      app_private_key_pem: string;
      alipay_public_key_pem: string;
    }
  | { provider: 'binance'; api_key: string; secret_key: string }
  | { provider: 'okx'; api_key: string; secret_key: string; passphrase: string }
  | { provider: 'ifind'; refresh_token: string };

export interface ConnectionStatus {
  provider: ProviderId;
  configured: boolean;
}

export interface AssetBalance {
  asset: string;
  free: string;
  locked: string;
}

export interface IfindQuote {
  symbol: string;
  quote_time: string | null;
  last: string;
}

export type ConnectionTestSummary =
  | {
      kind: 'bill_read';
      data: { bill_date: string; downloaded_bytes: number; hash_verified: boolean | null };
    }
  | { kind: 'binance_balances'; data: { balances: AssetBalance[] } }
  | { kind: 'okx_balances'; data: { balances: AssetBalance[] } }
  | { kind: 'ifind_quote'; data: { quote: IfindQuote } };

export type ConnectionErrorCode =
  | 'invalid_credentials'
  | 'permission_denied'
  | 'bill_unavailable'
  | 'data_unavailable'
  | 'rate_limited'
  | 'timeout'
  | 'network_failure'
  | 'invalid_response'
  | 'hash_mismatch'
  | 'secure_storage_unavailable'
  | 'unsupported_provider'
  | 'invalid_request'
  | 'provider_failure';

export interface ConnectionTestRequest {
  provider: ProviderId;
  bill_date?: string;
  symbol?: string;
}

export interface ConnectionTestResult {
  provider: ProviderId;
  outcome: 'connected' | 'failed';
  summary: ConnectionTestSummary | null;
  error_code: ConnectionErrorCode | null;
}

export const connectionsApi = {
  status: () => invoke<ConnectionStatus[]>('provider_connections'),
  save: (input: CredentialInput) =>
    invoke<ConnectionStatus>('save_provider_credentials', { input }),
  remove: (provider: ProviderId) =>
    invoke<ConnectionStatus>('remove_provider_credentials', { provider }),
  test: (request: ConnectionTestRequest) =>
    invoke<ConnectionTestResult>('connections_test', { request }),
};
