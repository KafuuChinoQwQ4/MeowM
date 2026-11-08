import { ProviderConnectionCard } from './ProviderConnectionCard';
import { useConnections } from './useConnections';
import type { ProviderId } from '../../api/connections';
import '../../styles/providerConnections.css';

const paymentProviders = [
  {
    provider: 'wechat_pay',
    title: '微信支付',
    fields: [
      { name: 'merchant_id', label: '商户号' },
      { name: 'certificate_serial', label: '证书序列号' },
      { name: 'private_key_pem', label: '商户私钥 PEM', multiline: true, secret: true },
    ],
  },
  {
    provider: 'alipay',
    title: '支付宝',
    fields: [
      { name: 'app_id', label: '商户应用 App ID' },
      { name: 'app_private_key_pem', label: '应用私钥 PEM', multiline: true, secret: true },
      { name: 'alipay_public_key_pem', label: '支付宝公钥 PEM', multiline: true },
    ],
  },
] as const;

export function PaymentConnectionsPage() {
  const connections = useConnections();
  const configured = (provider: ProviderId) =>
    connections.statuses.find((status) => status.provider === provider)?.configured ?? false;

  return (
    <section className="provider-connections-page" aria-labelledby="payment-connections-title">
      <header className="provider-connections-header">
        <h2 id="payment-connections-title">支付 API</h2>
      </header>
      {connections.loading && <p role="status">正在读取连接状态…</p>}
      {connections.error && (
        <p className="form-error" role="alert">
          {connections.error}
        </p>
      )}
      <div className="provider-connections-list">
        {paymentProviders.map((provider) => (
          <ProviderConnectionCard
            key={provider.provider}
            provider={provider.provider}
            title={provider.title}
            fields={[...provider.fields]}
            configured={configured(provider.provider)}
            operation="bill"
            onSave={connections.save}
            onRemove={connections.remove}
            onTest={connections.test}
          />
        ))}
      </div>
    </section>
  );
}
