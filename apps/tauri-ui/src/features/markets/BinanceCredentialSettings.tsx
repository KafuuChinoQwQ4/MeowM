import type { ConnectionTestResult, CredentialInput, ProviderId } from '../../api/connections';
import { ProviderConnectionCard } from '../connections/ProviderConnectionCard';

const scopes: Array<{
  provider: Extract<
    ProviderId,
    'binance' | 'binance_trade_demo' | 'binance_trade_live' | 'binance_withdraw'
  >;
  title: string;
  operation: 'balances';
}> = [
  {
    provider: 'binance',
    title: '币安只读 / 公共',
    operation: 'balances',
  },
  {
    provider: 'binance_trade_demo',
    title: '币安交易 · Testnet',
    operation: 'balances',
  },
  {
    provider: 'binance_trade_live',
    title: '币安交易 · 生产',
    operation: 'balances',
  },
  {
    provider: 'binance_withdraw',
    title: '币安提现',
    operation: 'balances',
  },
];

export function BinanceCredentialSettings({
  configured,
  onSave,
  onRemove,
  onTest,
  onBack,
}: {
  configured: (provider: ProviderId) => boolean;
  onSave: (input: CredentialInput) => Promise<void>;
  onRemove: (provider: ProviderId) => Promise<void>;
  onTest: (
    provider: ProviderId,
    options: { bill_date?: string; symbol?: string },
  ) => Promise<ConnectionTestResult>;
  onBack: () => void;
}) {
  return (
    <div className="provider-connection-settings">
      <p className="field-hint">每个权限范围请使用独立的 API Key。</p>
      {scopes.map((scope) => (
        <ProviderConnectionCard
          key={scope.provider}
          provider={scope.provider}
          title={scope.title}
          fields={[
            { name: 'api_key', label: 'API Key' },
            { name: 'secret_key', label: 'Secret Key' },
          ]}
          configured={configured(scope.provider)}
          operation={scope.operation}
          onSave={onSave}
          onRemove={onRemove}
          onTest={onTest}
        />
      ))}
      <button className="button" type="button" onClick={onBack}>
        返回币安
      </button>
    </div>
  );
}
