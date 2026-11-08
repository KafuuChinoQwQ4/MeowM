import { useCallback, useState } from 'react';
import { CURRENCIES } from '../../api/accounts';
import { exchangeRatesApi, type ExchangeRateSyncState } from '../../api/exchangeRates';
import { LoadState } from '../../components/LoadState';
import { recentDaysRange } from '../../shared/calendar';
import { convertCurrencyAmount } from '../../shared/currencyConversion';
import { errorText } from '../../shared/errors';
import { currencyExponent, formatCurrency, today, validAmount } from '../../shared/format';
import { useResource } from '../../shared/useResource';
import { RateChart } from './RateChart';

const statusLabels: Record<ExchangeRateSyncState['status'], string> = {
  never: '尚未同步',
  success: '更新成功',
  no_new_data: '已检查，暂无新数据',
  network_error: '网络不可用，请检查网络后重试',
  timeout: '请求超时，请稍后重试',
  provider_error: '数据源暂时不可用，请稍后重试',
  invalid_data: '数据校验失败，请稍后重试',
};

export function ExchangeRatesPage() {
  const [base, setBase] = useState('CNY');
  const [quote, setQuote] = useState('USD');
  const [amount, setAmount] = useState('1');
  const [days, setDays] = useState(90);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [anchor, setAnchor] = useState(today);
  const [from, until] = recentDaysRange(anchor, days);
  const history = useResource(
    useCallback(
      () => exchangeRatesApi.pairHistory(base, quote, from, until),
      [base, quote, from, until],
    ),
  );
  const sync = useResource(useCallback(() => exchangeRatesApi.state(), []));
  const amountValid = validAmount(amount, false, currencyExponent(base));

  async function refresh() {
    if (refreshing) return;
    setRefreshing(true);
    setError('');
    setNotice('');
    try {
      const result = await exchangeRatesApi.refresh();
      sync.reload();
      if (result.status !== 'success' && result.status !== 'no_new_data') {
        setError(statusLabels[result.status]);
      } else {
        setAnchor(today());
        history.reload();
        setNotice(statusLabels[result.status]);
      }
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setRefreshing(false);
    }
  }

  const latest = history.data?.[history.data.length - 1];
  const convertedAmount = amountValid
    ? base === quote
      ? convertCurrencyAmount(amount, base, quote, '1', '1')
      : latest
        ? convertCurrencyAmount(
            amount,
            base,
            quote,
            latest.base_reference_rate,
            latest.quote_reference_rate,
          )
        : null
    : null;

  return (
    <section className="analytics-page exchange-rates-page" aria-labelledby="exchange-rates-title">
      <div className="analytics-row exchange-rates-heading">
        <div>
          <h2 id="exchange-rates-title">法币汇率</h2>
        </div>
        <button
          className="button button-primary"
          type="button"
          disabled={refreshing}
          onClick={() => void refresh()}
        >
          {refreshing ? '正在更新…' : '刷新汇率'}
        </button>
      </div>

      <div className="report-filters exchange-rates-controls">
        <label className="form-field">
          源币种
          <select value={base} onChange={(event) => setBase(event.target.value)}>
            {CURRENCIES.map(([code, label]) => (
              <option key={code} value={code}>
                {code} · {label}
              </option>
            ))}
          </select>
        </label>
        <label className="form-field">
          目标币种
          <select value={quote} onChange={(event) => setQuote(event.target.value)}>
            {CURRENCIES.map(([code, label]) => (
              <option key={code} value={code}>
                {code} · {label}
              </option>
            ))}
          </select>
        </label>
      </div>

      <div className="exchange-rate-converter">
        <label className="form-field">
          换算金额
          <input
            inputMode="decimal"
            maxLength={40}
            value={amount}
            onChange={(event) => setAmount(event.target.value)}
            aria-invalid={!amountValid}
          />
        </label>
        <div className="exchange-rate-converted">
          <span>换算结果</span>
          <strong>{convertedAmount === null ? '—' : formatCurrency(convertedAmount, quote)}</strong>
        </div>
      </div>
      {!amountValid && <p role="alert">请输入有效金额，最多 {currencyExponent(base)} 位小数。</p>}
      {amountValid && base !== quote && !latest && history.data && (
        <p className="muted">所选日期范围没有这组币种的共同汇率观察。</p>
      )}

      <div className="exchange-rate-value">
        {latest ? (
          <>
            <span>最新参考汇率</span>
            <strong>
              1 {base} = {latest.rate} {quote}
            </strong>
            <span>有效日期 {latest.effective_date}</span>
          </>
        ) : (
          <>
            <span>参考汇率</span>
            <strong>
              {base === quote ? '同币种换算比例为 1' : `1 ${base} 对应的 ${quote} 数量`}
            </strong>
          </>
        )}
      </div>

      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <section className="analytics-section exchange-rate-chart" aria-label="汇率趋势图">
        <h3>历史走势</h3>
        <div className="chart-switch exchange-rate-periods" role="group" aria-label="时间范围">
          {[7, 30, 90, 180, 365].map((range) => (
            <button
              key={range}
              type="button"
              aria-pressed={days === range}
              onClick={() => setDays(range)}
            >
              近{range}天
            </button>
          ))}
        </div>
        <p className="field-hint">
          {from} 至 {anchor}
        </p>
        <LoadState loading={history.loading} error={history.error} reload={history.reload} />
        {history.data &&
          (base === quote ? (
            <p className="empty-state">同一币种按 1:1 换算，无需汇率曲线。</p>
          ) : (
            <RateChart
              key={`${base}-${quote}-${days}`}
              rows={history.data}
              base={base}
              quote={quote}
            />
          ))}
      </section>

      <section className="analytics-section exchange-rate-source" aria-label="汇率数据说明">
        <h3>数据与同步</h3>
        <p className="field-hint">
          数据源：Frankfurter 汇总参考数据。交叉汇率由双方同日 CNY 参考价计算
          {latest?.provider_sources.length ? `（${latest.provider_sources.join('、')}）` : ''}
          ；参考汇率，非实时交易报价。
        </p>
        <LoadState loading={sync.loading} error={sync.error} reload={sync.reload} />
        {sync.data && (
          <p className="field-hint">
            同步状态：{statusLabels[sync.data.status]}
            {sync.data.last_attempt_utc && (
              <> · 最近检查：{new Date(sync.data.last_attempt_utc).toLocaleString('zh-CN')}</>
            )}
          </p>
        )}
      </section>
    </section>
  );
}
