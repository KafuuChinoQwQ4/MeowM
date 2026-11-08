import { marketApi, marketRequestId, type MarketJson } from '../../../api/marketApi';

export interface MarketCandle {
  timestamp: number;
  open: string;
  high: string;
  low: string;
  close: string;
  volume: string | null;
  confirmed: boolean | null;
}

export function object(value: MarketJson | undefined): { [key: string]: MarketJson } {
  return value && typeof value === 'object' && !Array.isArray(value) ? value : {};
}

export function array(value: MarketJson | undefined): MarketJson[] {
  return Array.isArray(value) ? value : [];
}

export function text(value: MarketJson | undefined): string {
  return typeof value === 'string' || typeof value === 'number' ? String(value) : '';
}

export function decimal(value: MarketJson | undefined): string | null {
  const result = text(value);
  return /^-?\d+(?:\.\d+)?$/.test(result) && Number.isFinite(Number(result)) ? result : null;
}

export function marketError(cause: unknown): string {
  const code = cause instanceof Error ? cause.message : String(cause);
  const labels: Record<string, string> = {
    invalid_credentials: 'API 凭据无效或已过期，请在 API 凭据设置中更新。',
    permission_denied: '当前 API 凭据缺少此数据的读取权限。',
    secure_storage_unavailable: '系统安全凭据存储不可用。',
    rate_limited: '请求频率受限，请稍后重试。',
    timeout: '请求超时，请重试。',
    network_failure: '无法连接行情服务，请检查网络后重试。',
    invalid_response: '行情服务返回的数据无法识别，请重试。',
    invalid_request: '请检查证券代码和查询范围。',
    crypto_session_changed: '凭据或账本会话已变更，请重新读取。',
  };
  return labels[code] ?? code;
}

export async function requestMarket(
  provider: 'binance' | 'ifind',
  endpoint: string,
  params: { [key: string]: MarketJson },
  credentials?: 'read',
): Promise<MarketJson> {
  try {
    const response = await marketApi[provider === 'binance' ? 'binanceRequest' : 'ifindRequest']({
      endpoint_id: endpoint,
      params,
      environment: 'production',
      ...(credentials ? { credentials } : {}),
      confirmed: false,
      request_id: marketRequestId(),
    });
    if (response.status !== 'success')
      throw new Error(
        `${response.message || marketError(response.code || '请求失败')}${response.code ? ` (${response.code})` : ''}`,
      );
    return response.data;
  } catch (cause) {
    throw new Error(marketError(cause));
  }
}
