import { useCallback, useState } from 'react';
import { cryptoApi, type CryptoPosition } from '../../../api/crypto';
import { useResource } from '../../../shared/useResource';
import { Amount, ReadState, safe, sumDecimals, time, useAction } from './shared';

export function PositionsPanel() {
  const [type, setType] = useState('SWAP');
  return (
    <div className="okx-panel">
      <label className="form-field">
        合约类型
        <select
          aria-label="合约类型"
          value={type}
          onChange={(event) => setType(event.target.value)}
        >
          <option value="SWAP">永续合约</option>
          <option value="FUTURES">交割合约</option>
        </select>
      </label>
      <PositionData key={type} type={type} />
    </div>
  );
}
function PositionData({ type }: { type: string }) {
  const state = useResource(useCallback(() => safe(cryptoApi.positions(type)), [type]));
  const [older, setOlder] = useState<CryptoPosition[]>([]);
  const [cursor, setCursor] = useState<string | null | undefined>(undefined);
  const action = useAction();
  const history = [
    ...new Map([...older, ...(state.data?.history ?? [])].map((r) => [r.position_id, r])).values(),
  ].sort((a, b) => b.updated_at - a.updated_at);
  const next = cursor === undefined ? state.data?.next_after : cursor;
  const currencies = [...new Set(history.map((r) => r.currency))];
  return (
    <>
      <button
        className="button"
        disabled={state.loading || action.busy}
        onClick={() => {
          setOlder([]);
          setCursor(undefined);
          state.reload();
        }}
      >
        刷新持仓
      </button>
      <ReadState {...state} />
      <p className="field-hint">
        历史持仓覆盖平台可查询的最近 3
        个月。已实现净盈亏已包含手续费、资金费、强平费用和结算盈亏，费用分项仅供核对。
      </p>
      {state.data && (
        <>
          <p className="field-hint">读取时间 {time(state.data.fetched_at)}</p>
          {currencies.length > 0 && (
            <section aria-label="投资表现">
              <h4>已读取历史净盈亏</h4>
              <dl className="okx-values">
                {currencies.map((currency) => {
                  const group = history.filter((r) => r.currency === currency);
                  const values = group.flatMap((r) =>
                    r.realized_pnl === null ? [] : [r.realized_pnl],
                  );
                  return (
                    <div key={currency}>
                      <dt>
                        {currency} · {values.length}/{group.length} 笔有净盈亏
                      </dt>
                      <dd>
                        <Amount
                          value={values.length ? sumDecimals(values) : null}
                          unit={currency}
                        />
                      </dd>
                    </div>
                  );
                })}
              </dl>
              <p className="field-hint">
                按结算币种分别统计已读取的持仓，缺失值不视为零；加载后续页可补充历史。含部分平仓记录。
              </p>
            </section>
          )}
          <section aria-label="当前持仓">
            <h4>当前持仓</h4>
            <div className="okx-card-list">
              {state.data.current.map((position) => (
                <PositionCard key={position.position_id} position={position} history={false} />
              ))}
            </div>
            {!state.data.current.length && <p>暂无当前持仓。</p>}
          </section>
          <section aria-label="历史持仓">
            <h4>历史持仓</h4>
            <div className="okx-card-list">
              {history.map((position) => (
                <PositionCard key={position.position_id} position={position} history />
              ))}
            </div>
            {!history.length && <p>最近 3 个月暂无历史持仓。</p>}
          </section>
          {next && (
            <button
              className="button"
              disabled={action.busy || state.loading}
              onClick={() =>
                void action.run(async () => {
                  const result = await cryptoApi.positions(type, next);
                  if (action.active.current) {
                    setOlder((rows) => [...rows, ...result.history]);
                    setCursor(result.next_after);
                  }
                })
              }
            >
              {action.busy ? '正在读取…' : '加载更早持仓'}
            </button>
          )}
        </>
      )}
      {action.error && <p role="alert">{action.error}</p>}
    </>
  );
}
function PositionCard({ position: p, history }: { position: CryptoPosition; history: boolean }) {
  const side =
    ({ long: '多仓', short: '空仓', net: '净仓' } as Record<string, string>)[p.side] ?? p.side;
  const entries: [string, string | null, string?][] = [
    [history ? '累计平仓张数' : '持仓张数', p.quantity],
    ['开仓均价', p.open_price],
    ...(history
      ? ([
          ['平仓均价', p.close_price],
          ['已实现净盈亏', p.realized_pnl, p.currency],
          ['交易盈亏（不含费用）', p.pnl, p.currency],
        ] as [string, string | null, string?][])
      : ([['未实现盈亏', p.unrealized_pnl, p.currency]] as [string, string | null, string?][])),
    ['手续费/返佣', p.fee, p.currency],
    ['资金费用', p.funding_fee, p.currency],
    ['强平费用', p.liquidation_penalty, p.currency],
  ];
  return (
    <article className="okx-record">
      <header>
        <strong>{p.instrument}</strong>
        <span>
          {side} ·{' '}
          {p.margin_mode === 'cross'
            ? '全仓'
            : p.margin_mode === 'isolated'
              ? '逐仓'
              : p.margin_mode}
        </span>
      </header>
      <dl className="okx-values">
        {entries.map(([label, value, unit]) => (
          <div key={label}>
            <dt>{label}</dt>
            <dd>
              <Amount value={value} unit={unit} />
            </dd>
          </div>
        ))}
      </dl>
      <small className="field-hint">
        更新 {time(p.updated_at)} · 持仓 {p.position_id}
      </small>
    </article>
  );
}
