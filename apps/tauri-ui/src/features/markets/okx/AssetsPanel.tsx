import { useCallback } from 'react';
import { cryptoApi } from '../../../api/crypto';
import { useResource } from '../../../shared/useResource';
import { Amount, ReadState, safe, time } from './shared';

export function AssetsPanel() {
  const state = useResource(
    useCallback(() => safe(cryptoApi.assets()), []),
    true,
  );
  return (
    <div className="okx-panel">
      <button className="button" disabled={state.loading} onClick={state.reload}>
        刷新资产
      </button>
      <ReadState {...state} />
      {state.data && (
        <>
          <div className="okx-valuation">
            <span>OKX 账户总估值 · USD</span>
            <strong>
              <Amount value={state.data.valuation.total_usd} />
            </strong>
            <small>估值时间 {time(state.data.valuation.timestamp)}</small>
          </div>
          <dl className="okx-values">
            <div>
              <dt>交易账户 · USD</dt>
              <dd>
                <Amount value={state.data.valuation.trading_usd} />
              </dd>
            </div>
            <div>
              <dt>资金账户 · USD</dt>
              <dd>
                <Amount value={state.data.valuation.funding_usd} />
              </dd>
            </div>
            <div>
              <dt>赚币账户 · USD</dt>
              <dd>
                <Amount value={state.data.valuation.earn_usd} />
              </dd>
            </div>
          </dl>
          <p className="field-hint">
            总估值由 OKX 提供，可能包含赚币等其他账户。合约持仓价值不重复加到总资产中。
          </p>
          {(['trading', 'funding'] as const).map((account) => (
            <section key={account} aria-label={account === 'trading' ? '交易账户' : '资金账户'}>
              <h4>{account === 'trading' ? '交易账户' : '资金账户'}</h4>
              <div className="okx-card-list">
                {state
                  .data!.balances.filter((b) => b.account === account)
                  .map((balance) => (
                    <article
                      className="okx-record"
                      key={balance.asset}
                      data-crypto-balance={`${account}:${balance.asset}`}
                    >
                      <strong>{balance.asset}</strong>
                      <dl className="okx-values">
                        <div>
                          <dt>数量</dt>
                          <dd>
                            <Amount value={balance.quantity} />
                          </dd>
                        </div>
                        <div>
                          <dt>可用</dt>
                          <dd>
                            <Amount value={balance.available} />
                          </dd>
                        </div>
                        <div>
                          <dt>冻结</dt>
                          <dd>
                            <Amount value={balance.frozen} />
                          </dd>
                        </div>
                        {account === 'trading' && (
                          <>
                            <div>
                              <dt>权益</dt>
                              <dd>
                                <Amount value={balance.equity} />
                              </dd>
                            </div>
                            <div>
                              <dt>权益估值 · USD</dt>
                              <dd>
                                <Amount value={balance.equity_usd} />
                              </dd>
                            </div>
                          </>
                        )}
                      </dl>
                    </article>
                  ))}
              </div>
              {!state.data!.balances.some((b) => b.account === account) && <p>暂无非零资产。</p>}
            </section>
          ))}
          <p className="field-hint">
            数量按原币显示；— 表示该账户模式未提供该值。读取时间 {time(state.data.fetched_at)}
          </p>
        </>
      )}
    </div>
  );
}
