import { useCallback, useState } from 'react';
import {
  cryptoApi,
  type CryptoHistoryRequest,
  type CryptoHistorySource,
  type CryptoRecord,
  type CryptoReviewStatus,
} from '../../../api/crypto';
import { useResource } from '../../../shared/useResource';
import { Amount, ConfigureHint, ReadState, safe, time, useAction } from './shared';

function localInput(value: number): string {
  const date = new Date(value);
  return new Date(value - date.getTimezoneOffset() * 60000).toISOString().slice(0, 19);
}
const statusLabels = { pending: '待确认', confirmed: '已确认', ignored: '已忽略' };
const sourceLabels = {
  bill: '账单变动（核对）',
  fill: '现货资产转换',
  deposit: '充值',
  withdrawal: '提现',
};
function confirmable(record: CryptoRecord) {
  return !['deposit', 'withdrawal'].includes(record.source) || record.platform_state === '2';
}
function upstream(record: CryptoRecord): string {
  if (record.source === 'fill') return '已成交';
  if (record.source === 'bill') return `账单类型 ${record.platform_state}`;
  return (
    (
      {
        '2': '成功',
        '0': '等待处理',
        '1': record.source === 'deposit' ? '已入账，待最终成功' : '链上处理中',
        '-1': '失败',
        '-2': '已取消',
        '-3': '取消中',
      } as Record<string, string>
    )[record.platform_state] ?? `平台状态 ${record.platform_state}`
  );
}

export function RecordsPanel({
  configured,
  onConfigure,
}: {
  configured: boolean;
  onConfigure: () => void;
}) {
  const [source, setSource] = useState<CryptoHistorySource>('bills');
  const [begin, setBegin] = useState(() => localInput(Date.now() - 6 * 86400000));
  const [end, setEnd] = useState(() => localInput(Date.now()));
  const [status, setStatus] = useState<CryptoReviewStatus | ''>('pending');
  const [offset, setOffset] = useState(0);
  const [nextRequest, setNextRequest] = useState<CryptoHistoryRequest | null>(null);
  const [notice, setNotice] = useState('');
  const rows = useResource(
    useCallback(() => safe(cryptoApi.records(status || null, offset)), [status, offset]),
  );
  const action = useAction();
  const resetPage = () => {
    setNextRequest(null);
    setNotice('');
  };
  const request = (): CryptoHistoryRequest => ({
    source,
    begin: Date.parse(begin),
    end: Date.parse(end),
    after: null,
    account_uid: null,
  });
  const validRange =
    Number.isFinite(Date.parse(begin)) &&
    Number.isFinite(Date.parse(end)) &&
    Date.parse(end) >= Date.parse(begin);
  const read = (input: CryptoHistoryRequest) =>
    action.run(async () => {
      const result = await cryptoApi.importHistory(input);
      if (!action.active.current) return;
      setNotice(
        `本页读取 ${result.fetched} 条，新增 ${result.inserted} 条，重复 ${result.duplicates} 条。${result.next_after ? '还有更早记录，请继续读取下一页。' : '所选范围已读取完毕。'}`,
      );
      setNextRequest(
        result.next_after
          ? { ...input, after: result.next_after, account_uid: result.account_uid }
          : null,
      );
      setOffset(0);
      rows.reload();
    });
  const review = (key: string, next: CryptoReviewStatus) =>
    action.run(async () => {
      await cryptoApi.review([key], next);
      if (action.active.current) {
        setNotice(`记录${statusLabels[next]}，已保存到加密资产记录。`);
        rows.reload();
      }
    });
  return (
    <div className="okx-panel">
      <p className="field-hint">
        候选需人工核对。现货买卖保留两种资产的转换和手续费，确认后保存在加密流水中，不计入法币收支。
      </p>
      {!configured && <ConfigureHint onConfigure={onConfigure} />}
      <form
        className="okx-import-form"
        onSubmit={(event) => {
          event.preventDefault();
          if (configured && validRange) void read(request());
        }}
      >
        <label className="form-field">
          流水来源
          <select
            aria-label="流水来源"
            value={source}
            disabled={action.busy}
            onChange={(event) => {
              setSource(event.target.value as CryptoHistorySource);
              resetPage();
            }}
          >
            <option value="bills">账户账单 · 最近 7 天</option>
            <option value="bills_archive">账户账单 · 最近 3 个月</option>
            <option value="fills">现货成交 · 最近 3 个月</option>
            <option value="deposits">充值历史</option>
            <option value="withdrawals">提现历史</option>
          </select>
        </label>
        <div className="okx-date-range">
          <label className="form-field">
            开始时间
            <input
              type="datetime-local"
              aria-label="开始时间"
              step="1"
              required
              value={begin}
              disabled={action.busy}
              onChange={(event) => {
                setBegin(event.target.value);
                resetPage();
              }}
            />
          </label>
          <label className="form-field">
            结束时间
            <input
              type="datetime-local"
              aria-label="结束时间"
              step="1"
              required
              value={end}
              disabled={action.busy}
              onChange={(event) => {
                setEnd(event.target.value);
                resetPage();
              }}
            />
          </label>
        </div>
        <p className="field-hint">
          默认读取最近 6 天。归档和成交可调整至最近 3 个月；充提本次支持读取最近 3 个月。
        </p>
        <div className="okx-actions">
          <button
            className="button button-primary"
            type="submit"
            disabled={!configured || action.busy || !validRange}
          >
            {action.busy ? '处理中…' : '读取并导入候选'}
          </button>
          {nextRequest && (
            <button
              className="button"
              type="button"
              disabled={!configured || action.busy}
              onClick={() => void read(nextRequest)}
            >
              读取下一页候选
            </button>
          )}
        </div>
      </form>
      {notice && (
        <p role="status" className="okx-notice">
          {notice}
        </p>
      )}
      {action.error && <p role="alert">{action.error}</p>}
      <div className="okx-controls">
        <label className="form-field">
          记录状态
          <select
            aria-label="记录状态"
            value={status}
            onChange={(event) => {
              setStatus(event.target.value as CryptoReviewStatus | '');
              setOffset(0);
            }}
          >
            <option value="pending">待确认</option>
            <option value="confirmed">已确认</option>
            <option value="ignored">已忽略</option>
            <option value="">全部记录</option>
          </select>
        </label>
        <button className="button" disabled={rows.loading} onClick={rows.reload}>
          刷新记录
        </button>
      </div>
      <ReadState {...rows} />
      {rows.data && (
        <>
          <p className="field-hint">共 {rows.data.total} 条 · 加密流水独立保存</p>
          <div className="okx-card-list">
            {rows.data.rows.map(({ record, status: reviewStatus }) => (
              <article className="okx-record" key={record.key} data-crypto-key={record.key}>
                <header>
                  <strong>
                    {sourceLabels[record.source]} {record.instrument}
                  </strong>
                  <span>{statusLabels[reviewStatus]}</span>
                </header>
                <p className="field-hint">
                  {time(record.occurred_at)} · {upstream(record)}
                </p>
                <div className="okx-legs">
                  {record.legs.map((leg, index) => (
                    <strong key={index}>
                      <Amount value={leg.quantity} unit={leg.asset} />
                    </strong>
                  ))}
                </div>
                {record.fee && (
                  <p>
                    手续费/返佣 <Amount value={record.fee.quantity} unit={record.fee.asset} />
                  </p>
                )}
                {record.price && (
                  <p>
                    成交价 <Amount value={record.price} unit={record.legs[1]?.asset} />
                  </p>
                )}
                {record.source === 'bill' && (
                  <p className="field-hint">
                    账单变动用于核对，可能与成交或充提描述同一事件，不重复汇总到投资盈亏。
                  </p>
                )}
                {!confirmable(record) && (
                  <p className="field-hint">
                    平台未成功完成该笔充提，暂不能确认。重新导入可更新平台状态。
                  </p>
                )}
                <small className="field-hint">
                  账户 UID {record.account_uid} · 来源编号 {record.source_id}
                </small>
                <div className="okx-actions">
                  {reviewStatus !== 'confirmed' && (
                    <button
                      className="button button-primary"
                      disabled={action.busy || !confirmable(record)}
                      onClick={() => void review(record.key, 'confirmed')}
                    >
                      确认加密记录
                    </button>
                  )}
                  {reviewStatus === 'pending' && (
                    <button
                      className="button"
                      disabled={action.busy}
                      onClick={() => void review(record.key, 'ignored')}
                    >
                      忽略记录
                    </button>
                  )}
                  {reviewStatus !== 'pending' && (
                    <button
                      className="button"
                      disabled={action.busy}
                      onClick={() => void review(record.key, 'pending')}
                    >
                      移回待确认
                    </button>
                  )}
                </div>
              </article>
            ))}
          </div>
          {rows.data.rows.length === 0 && (
            <p>暂无{status ? statusLabels[status] : ''}的加密记录。</p>
          )}
          <div className="okx-actions">
            <button
              className="button"
              disabled={offset === 0 || rows.loading}
              onClick={() => setOffset((v) => Math.max(0, v - 50))}
            >
              上一页记录
            </button>
            <button
              className="button"
              disabled={offset + 50 >= rows.data.total || rows.loading}
              onClick={() => setOffset((v) => v + 50)}
            >
              下一页记录
            </button>
          </div>
        </>
      )}
    </div>
  );
}
