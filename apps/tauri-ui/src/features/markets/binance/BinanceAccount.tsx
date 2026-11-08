import { useCallback } from 'react';
import { useResource } from '../../../shared/useResource';
import { displayAmount } from '../../../shared/format';
import { binanceBalances } from './api';
import { ReadState } from '../shared/ReadState';

export function BinanceAccount() {
  const balances = useResource(useCallback(binanceBalances, []));
  return (
    <div className="okx-panel">
      <h4>现货账户余额</h4>
      <button
        className="button"
        type="button"
        disabled={balances.loading}
        onClick={balances.reload}
      >
        刷新账户
      </button>
      <ReadState {...balances} />
      {balances.data && !balances.data.length && <p>现货账户暂无非零余额。</p>}
      <div className="okx-card-list">
        {balances.data?.map((row) => (
          <article className="okx-record" key={row.asset}>
            <strong>{row.asset}</strong>
            <dl className="okx-values">
              <div>
                <dt>可用</dt>
                <dd>{displayAmount(row.free)}</dd>
              </div>
              <div>
                <dt>冻结</dt>
                <dd>{displayAmount(row.locked)}</dd>
              </div>
            </dl>
          </article>
        ))}
      </div>
    </div>
  );
}
