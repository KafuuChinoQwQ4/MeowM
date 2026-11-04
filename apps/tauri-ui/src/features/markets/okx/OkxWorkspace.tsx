import { useState } from 'react';
import { MarketPanel } from './MarketPanel';
import { AssetsPanel } from './AssetsPanel';
import { RecordsPanel } from './RecordsPanel';
import { PositionsPanel } from './PositionsPanel';
import { ConfigureHint } from './shared';
import '../../../styles/okx.css';

const tabs = {
  market: 'OKX 行情',
  assets: '资产总览',
  records: '加密流水',
  positions: '合约表现',
} as const;
export function OkxWorkspace({ configured }: { configured: boolean }) {
  const [tab, setTab] = useState<keyof typeof tabs>('market');
  return (
    <section className="okx-workspace" aria-label="OKX 加密资产">
      <header>
        <h3>OKX 加密资产</h3>
        <p className="field-hint">查看行情、账户和投资记录，独立管理加密资产流水。</p>
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
        key={`${tab}-${configured}`}
      >
        {tab === 'market' && <MarketPanel />}
        {tab === 'records' && <RecordsPanel configured={configured} />}
        {tab === 'assets' && (configured ? <AssetsPanel /> : <ConfigureHint />)}
        {tab === 'positions' && (configured ? <PositionsPanel /> : <ConfigureHint />)}
      </div>
    </section>
  );
}
