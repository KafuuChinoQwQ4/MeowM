import { ProviderConnectionCard } from '../connections/ProviderConnectionCard';
import { useConnections } from '../connections/useConnections';
import type { ProviderId } from '../../api/connections';
import '../../styles/providerConnections.css';

const marketProviders = [
  {
    provider: 'binance',
    title: '币安',
    description: '读取现货账户非零余额。API Key 请仅开启读取权限。',
    operation: 'balances',
    fields: [
      { name: 'api_key', label: 'API Key' },
      { name: 'secret_key', label: 'Secret Key' },
    ],
  },
  {
    provider: 'okx',
    title: '欧易',
    description: '读取账户余额。API Key 请仅开启读取权限。',
    operation: 'balances',
    fields: [
      { name: 'api_key', label: 'API Key' },
      { name: 'secret_key', label: 'Secret Key' },
      { name: 'passphrase', label: 'Passphrase' },
    ],
  },
  {
    provider: 'ifind',
    title: '同花顺 iFinD',
    description: '行情数据服务，不连接证券账户或下单功能。',
    operation: 'quote',
    fields: [{ name: 'refresh_token', label: 'HTTP API Refresh Token' }],
  },
] as const;

export function MarketsPage() {
  const connections = useConnections();
  const configured = (provider: ProviderId) =>
    connections.statuses.find((status) => status.provider === provider)?.configured ?? false;

  return (
    <section className="provider-connections-page" aria-labelledby="markets-title">
      <header className="provider-connections-header">
        <h2 id="markets-title">交易所</h2>
        <p>交易所仅读取余额，iFinD 用于读取行情。此页面不提供买卖、转账或提现操作。</p>
      </header>
      {connections.loading && <p role="status">正在读取连接状态…</p>}
      {connections.error && (
        <p className="form-error" role="alert">
          {connections.error}
        </p>
      )}
      <div className="provider-connections-list">
        {marketProviders.map((provider) => (
          <ProviderConnectionCard
            key={provider.provider}
            provider={provider.provider}
            title={provider.title}
            description={provider.description}
            fields={[...provider.fields]}
            configured={configured(provider.provider)}
            operation={provider.operation}
            onSave={connections.save}
            onRemove={connections.remove}
            onTest={connections.test}
          />
        ))}
        <article className="provider-connection-placeholder">
          <h3>更多交易所</h3>
          <p>更多交易所，后续接入</p>
          <span>尚未开放</span>
        </article>
      </div>
    </section>
  );
}
