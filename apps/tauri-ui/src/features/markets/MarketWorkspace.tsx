import { useState } from 'react';
import { MarketApiWorkspace } from './MarketApiWorkspace';
import { BinanceMarket } from './binance/BinanceMarket';
import { BinanceAccount } from './binance/BinanceAccount';
import { IfindMarket } from './ifind/IfindMarket';
import '../../styles/okx.css';

export function MarketWorkspace({
  provider,
  configured,
  configuredScopes,
  onConfigure,
}: {
  provider: 'binance' | 'ifind';
  configured: boolean;
  configuredScopes?: { read: boolean; trade_demo: boolean; trade_live: boolean; withdraw: boolean };
  onConfigure: () => void;
}) {
  const [tab, setTab] = useState('market');
  const title = provider === 'binance' ? '币安' : '同花顺 iFinD';
  const tabs = [
    ['market', provider === 'binance' ? '币安行情' : '股票行情'],
    ['favorites', '自选'],
    ...(provider === 'binance' ? [['assets', '资产总览']] : []),
    ['api', '更多接口'],
  ];
  return (
    <section className="okx-workspace" aria-label={`${title} 行情工作区`}>
      <header>
        <h3>{title}行情</h3>
        <button className="button" type="button" onClick={onConfigure}>
          API 凭据设置
        </button>
      </header>
      <div className="okx-tabs" role="tablist" aria-label={`${title} 功能`}>
        {tabs.map(([key, label]) => (
          <button
            key={key}
            id={`${provider}-tab-${key}`}
            role="tab"
            aria-selected={tab === key}
            aria-controls={`${provider}-panel`}
            onClick={() => setTab(key)}
          >
            {label}
          </button>
        ))}
      </div>
      <div role="tabpanel" id={`${provider}-panel`} aria-labelledby={`${provider}-tab-${tab}`}>
        {(tab === 'market' || tab === 'favorites') &&
          (provider === 'binance' ? (
            <BinanceMarket favoritesOnly={tab === 'favorites'} />
          ) : (
            <IfindMarket
              key={String(configured)}
              configured={configured}
              favoritesOnly={tab === 'favorites'}
              onConfigure={onConfigure}
            />
          ))}
        {tab === 'assets' &&
          (configured ? (
            <BinanceAccount key={String(configured)} />
          ) : (
            <p className="field-hint">
              请配置币安只读凭据以读取账户数据。{' '}
              <button className="text-button" type="button" onClick={onConfigure}>
                配置只读凭据
              </button>
            </p>
          ))}
        {tab === 'api' && (
          <MarketApiWorkspace provider={provider} configuredScopes={configuredScopes} />
        )}
      </div>
    </section>
  );
}
