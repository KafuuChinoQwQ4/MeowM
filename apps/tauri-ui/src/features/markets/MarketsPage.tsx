import { ProviderConnectionCard } from '../connections/ProviderConnectionCard';
import { useEffect, useRef } from 'react';
import { useConnections } from '../connections/useConnections';
import { OkxWorkspace } from './okx/OkxWorkspace';
import { OkxCredentialSettings } from './okx/OkxCredentialSettings';
import type { CredentialInput, ProviderId } from '../../api/connections';
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
    description: '公开行情无需 API Key；账户数据可配置只读凭据，交易功能使用单独的交易凭据。',
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

export type MarketRoute = {
  provider: (typeof marketProviders)[number]['provider'];
  page: 'account' | 'settings';
} | null;

export function MarketsPage({
  route,
  onNavigate,
}: {
  route: MarketRoute;
  onNavigate: (route: MarketRoute) => void;
}) {
  const connections = useConnections();
  const currentRoute = useRef(route);
  const heading = useRef<HTMLHeadingElement>(null);
  currentRoute.current = route;
  useEffect(
    () => () => {
      currentRoute.current = null;
    },
    [],
  );
  useEffect(() => {
    const content = heading.current?.closest('#day-content');
    if (content) content.scrollTop = 0;
    heading.current?.focus({ preventScroll: true });
  }, [route]);
  const provider = marketProviders.find((item) => item.provider === route?.provider);
  const configured = (id: ProviderId) =>
    connections.statuses.find((status) => status.provider === id)?.configured ?? false;
  const needsSetup =
    provider &&
    (provider.provider === 'okx'
      ? route?.page === 'settings'
      : !configured(provider.provider) || route?.page === 'settings');
  const unavailable = connections.loading || !!connections.error;
  const routeUnavailable = provider?.provider === 'okx' ? connections.loading : unavailable;
  const save = async (input: CredentialInput) => {
    const origin = route;
    await connections.save(input);
    if (origin && currentRoute.current === origin) onNavigate({ ...origin, page: 'account' });
  };
  const remove = async (id: ProviderId) => {
    const origin = route;
    await connections.remove(id);
    if (origin && currentRoute.current === origin) onNavigate({ ...origin, page: 'account' });
  };

  return (
    <section className="provider-connections-page" aria-labelledby="markets-title">
      <header className="provider-connections-header">
        <h2 id="markets-title" ref={heading} tabIndex={-1}>
          {provider
            ? `${provider.title}${route?.page === 'settings' ? ' · API 设置' : ''}`
            : '交易所'}
        </h2>
        <p>
          {provider
            ? needsSetup
              ? provider.provider === 'okx'
                ? '按需分别配置只读、模拟盘和实盘 API 凭据。'
                : '填写 API 凭据，保存后进入该交易所。'
              : provider.description
            : '选择交易所，查看账户与行情。OKX 公开行情可直接查看。'}
        </p>
      </header>
      {connections.loading && <p role="status">正在读取连接状态…</p>}
      {connections.error && (
        <p className="form-error" role="alert">
          {connections.error}{' '}
          <button className="text-button" onClick={() => void connections.reload()}>
            重试
          </button>
        </p>
      )}
      {!provider ? (
        <div className="provider-connections-list">
          {marketProviders.map((item) => (
            <button
              className="directory-entry market-provider-entry"
              key={item.provider}
              type="button"
              aria-label={`打开${item.title}`}
              disabled={connections.loading || (!!connections.error && item.provider !== 'okx')}
              onClick={() => onNavigate({ provider: item.provider, page: 'account' })}
            >
              <span>
                <strong>{item.title}</strong>
                <small>{item.description}</small>
              </span>
              <span
                className={
                  configured(item.provider) ? 'connection-state configured' : 'connection-state'
                }
              >
                {connections.loading
                  ? '读取状态中'
                  : connections.error
                    ? '状态不可用'
                    : configured(item.provider)
                      ? '已配置'
                      : '待配置 API'}
              </span>
              <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
                <path d="m9 5 7 7-7 7" />
              </svg>
            </button>
          ))}
          <article className="provider-connection-placeholder">
            <h3>更多交易所</h3>
            <p>更多交易所，后续接入</p>
            <span>尚未开放</span>
          </article>
        </div>
      ) : !routeUnavailable ? (
        <>
          {!needsSetup && provider.provider !== 'okx' && (
            <div>
              <button
                className="button"
                type="button"
                onClick={() => onNavigate({ provider: provider.provider, page: 'settings' })}
              >
                API 设置
              </button>
            </div>
          )}
          {provider.provider === 'okx' ? (
            needsSetup ? (
              <OkxCredentialSettings
                configured={configured}
                onSave={save}
                onRemove={remove}
                onBack={() => onNavigate({ provider: 'okx', page: 'account' })}
              />
            ) : (
              <OkxWorkspace
                readConfigured={configured('okx')}
                demoConfigured={configured('okx_trade_demo')}
                liveConfigured={configured('okx_trade_live')}
                onCredentialSettings={() => onNavigate({ provider: 'okx', page: 'settings' })}
              />
            )
          ) : (
            <ProviderConnectionCard
              key={`${provider.provider}-${needsSetup ? 'settings' : 'account'}`}
              provider={provider.provider}
              title={provider.title}
              description={provider.description}
              fields={[...provider.fields]}
              configured={configured(provider.provider)}
              showCredentials={!!needsSetup}
              operation={provider.operation}
              onSave={save}
              onRemove={remove}
              onTest={connections.test}
            />
          )}
        </>
      ) : null}
    </section>
  );
}
