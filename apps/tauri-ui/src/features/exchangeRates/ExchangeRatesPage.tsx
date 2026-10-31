import { useCallback, useState } from 'react';
import { exchangeRatesApi, type ExchangeRateSyncState } from '../../api/exchangeRates';
import { LoadState } from '../../components/LoadState';
import { recentDaysRange } from '../../shared/calendar';
import { errorText } from '../../shared/errors';
import { today } from '../../shared/format';
import { useResource } from '../../shared/useResource';
import { RateChart } from './RateChart';

const currencies = [
  ['USD', '美元'],
  ['EUR', '欧元'],
  ['JPY', '日元'],
  ['HKD', '港币'],
  ['GBP', '英镑'],
  ['KRW', '韩元'],
  ['AUD', '澳元'],
  ['CAD', '加元'],
  ['SGD', '新加坡元'],
  ['CHF', '瑞士法郎'],
];
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
  const [quote, setQuote] = useState('USD');
  const [days, setDays] = useState(90);
  const [refreshing, setRefreshing] = useState(false);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [anchor, setAnchor] = useState(today);
  const [from, until] = recentDaysRange(anchor, days);
  const history = useResource(
    useCallback(() => exchangeRatesApi.history(quote, from, until), [quote, from, until]),
  );
  const sync = useResource(useCallback(() => exchangeRatesApi.state(), []));
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
  return (
    <section className="analytics-page exchange-rates-page" aria-labelledby="exchange-rates-title">
      <div className="analytics-row exchange-rates-heading">
        <div>
          <h2 id="exchange-rates-title">法币汇率</h2>
          <p className="field-hint">
            主要法定货币的每日参考汇率与历史走势，本地保存并支持离线查看。
          </p>
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
          目标币种
          <select value={quote} onChange={(event) => setQuote(event.target.value)}>
            {currencies.map(([code, label]) => (
              <option key={code} value={code}>
                {code} · {label}
              </option>
            ))}
          </select>
        </label>
        <label className="form-field">
          时间范围
          <select value={days} onChange={(event) => setDays(Number(event.target.value))}>
            {[30, 90, 365].map((range) => (
              <option key={range} value={range}>
                最近 {range} 日
              </option>
            ))}
          </select>
        </label>
      </div>
      <div className="exchange-rate-value">
        {latest ? (
          <>
            <span>最新参考汇率</span>
            <strong>
              1 CNY = {latest.rate} {quote}
            </strong>
            <span>有效日期 {latest.effective_date}</span>
          </>
        ) : (
          <>
            <span>参考汇率</span>
            <strong>1 CNY 对应的 {quote} 数量</strong>
          </>
        )}
      </div>
      {error && <p role="alert">{error}</p>}
      {notice && <p role="status">{notice}</p>}
      <LoadState loading={history.loading} error={history.error} reload={history.reload} />
      {history.data && (
        <section className="analytics-section exchange-rate-chart" aria-label="汇率趋势图">
          <h3>历史走势</h3>
          <RateChart key={`${quote}-${days}`} rows={history.data} quote={quote} />
        </section>
      )}
      <section className="analytics-section exchange-rate-source" aria-label="汇率数据说明">
        <h3>数据与同步</h3>
        <p className="field-hint">
          数据源：Frankfurter 汇总参考数据
          {latest?.provider_sources.length ? `（${latest.provider_sources.join('、')}）` : ''}
          。参考汇率，非实时交易报价。
        </p>
        <p className="field-hint">周末和节假日可能暂无新数据，曲线保留来源实际发布的日期。</p>
        <LoadState loading={sync.loading} error={sync.error} reload={sync.reload} />
        {sync.data && (
          <p className="field-hint">
            同步状态：{statusLabels[sync.data.status]}
            {sync.data.last_attempt_utc && (
              <> · 最近检查：{new Date(sync.data.last_attempt_utc).toLocaleString('zh-CN')}</>
            )}
          </p>
        )}
        <p className="field-hint">
          Android
          关闭界面后会在联网时每日尝试更新，实际时间受系统省电策略影响；桌面版在应用启动和手动刷新时更新。
        </p>
      </section>
    </section>
  );
}
