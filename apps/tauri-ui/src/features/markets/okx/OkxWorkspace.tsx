import { useState } from 'react';
import { MarketPanel } from './MarketPanel';
import { AssetsPanel } from './AssetsPanel';
import { RecordsPanel } from './RecordsPanel';
import { PositionsPanel } from './PositionsPanel';
import { TradingPanel } from './TradingPanel';
import { ConfigureHint } from './shared';
import '../../../styles/okx.css';

const tabs = {
  market: 'OKX 行情',
  assets: '资产总览',
  records: '加密流水',
  positions: '合约表现',
  trading: '交易下单',
} as const;
export function OkxWorkspace({
  readConfigured,
  demoConfigured,
  liveConfigured,
  onCredentialSettings,
}: {
  readConfigured: boolean;
  demoConfigured: boolean;
  liveConfigured: boolean;
  onCredentialSettings: () => void;
}) {
  const [tab, setTab] = useState<keyof typeof tabs>('market');
  return (
    <section className="okx-workspace" aria-label="OKX 加密资产">
      <header>
        <h3>OKX 加密资产</h3>
        <p className="field-hint">公开行情无需 API Key；账户数据和交易功能可按需配置各自的凭据。</p>
        <button className="button" type="button" onClick={onCredentialSettings}>
          API 凭据设置
        </button>
      </header>
      <div role="tablist" aria-label="OKX 功能" className="okx-tabs">
        {Object.entries(tabs).map(([key, label]) => (
          <button
            key={key}
            id={`okx-tab-${key}`}
            role="tab"
            aria-selected={tab === key}
            aria-controls="okx-panel"
            onClick={() => setTab(key as keyof typeof tabs)}
          >
            {label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id="okx-panel"
        aria-labelledby={`okx-tab-${tab}`}
        key={`${tab}-${readConfigured}-${demoConfigured}-${liveConfigured}`}
      >
        {tab === 'market' && <MarketPanel />}
        {tab === 'records' && (
          <RecordsPanel configured={readConfigured} onConfigure={onCredentialSettings} />
        )}
        {tab === 'assets' &&
          (readConfigured ? <AssetsPanel /> : <ConfigureHint onConfigure={onCredentialSettings} />)}
        {tab === 'positions' &&
          (readConfigured ? (
            <PositionsPanel />
          ) : (
            <ConfigureHint onConfigure={onCredentialSettings} />
          ))}
        {tab === 'trading' && (
          <TradingPanel
            demoConfigured={demoConfigured}
            liveConfigured={liveConfigured}
            onConfigure={onCredentialSettings}
          />
        )}
      </div>
    </section>
  );
}
