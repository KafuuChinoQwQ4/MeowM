import { useState } from 'react';
import { privacyApi, type DeletePreview, type PrivacyStatus } from '../../api/privacy';
import { FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
export function DeleteData({
  onStatus,
  onDeleted,
  onBackup,
}: {
  onStatus: (v: PrivacyStatus) => void;
  onDeleted: (scope: 'notifications' | 'all') => void;
  onBackup: () => void;
}) {
  const [preview, setPreview] = useState<DeletePreview | null>(null);
  const [phrase, setPhrase] = useState('');
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  const [message, setMessage] = useState('');
  const required = preview?.scope === 'all' ? '删除全部数据' : '删除通知数据';
  async function inspect(scope: 'notifications' | 'all') {
    setPreview(null);
    setPhrase('');
    setError('');
    setMessage('');
    setBusy(true);
    try {
      setPreview(await privacyApi.previewDelete(scope));
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="data-card" aria-label="删除数据">
      <h3>删除数据</h3>
      <p>先检查范围，再输入确认短语。建议先创建加密备份；删除完成后不能撤销。</p>
      <div className="delete-data-actions">
        <button className="button" onClick={onBackup}>
          前往创建加密备份
        </button>
        <div className="data-actions">
          <button className="button" disabled={busy} onClick={() => void inspect('notifications')}>
            预览删除通知数据
          </button>
          <button
            className="button button-danger"
            disabled={busy}
            onClick={() => void inspect('all')}
          >
            预览删除全部数据
          </button>
        </div>
      </div>
      {preview && (
        <div className="restore-preview">
          {preview.scope === 'notifications' ? (
            <>
              <p>
                待确认或已忽略候选：{preview.pending} 条；已确认交易：{preview.confirmed}{' '}
                条；通知去重收据：{preview.receipts} 条。
              </p>
              <p>
                清理包含通知和屏幕识别：关闭全部来源、停止屏幕识别、删除采集摘要和未确认候选；已确认交易及其分录保留，并留下来源清理审计。
              </p>
              {preview.blocked > 0 && (
                <p role="alert">
                  有 {preview.blocked}{' '}
                  条候选被历史对账或其他记录引用，不能单独删除。可先处理关联记录，或选择删除全部数据。
                </p>
              )}
            </>
          ) : (
            <>
              <p>
                将清空全部账本、规则、收据、对账和本机应用锁，并重建微信、支付宝、现金三个空平台。
              </p>
              <div className="data-counts">
                <table>
                  <thead>
                    <tr>
                      <th>数据表</th>
                      <th>将删除的记录</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.counts
                      .filter(([, count]) => count > 0)
                      .map(([name, count]) => (
                        <tr key={name}>
                          <th>{name}</th>
                          <td>{count}</td>
                        </tr>
                      ))}
                  </tbody>
                </table>
              </div>
            </>
          )}
          <label>
            输入“{required}”确认
            <input
              value={phrase}
              disabled={busy}
              onChange={(e) => setPhrase(e.target.value)}
              autoComplete="off"
            />
          </label>
          <div className="data-actions">
            <button
              className="button"
              disabled={busy}
              onClick={() => {
                setPreview(null);
                setPhrase('');
              }}
            >
              取消删除
            </button>
            <button
              className="button button-danger"
              disabled={busy || phrase !== required || preview.blocked > 0}
              onClick={async () => {
                setBusy(true);
                setError('');
                try {
                  const status = await privacyApi.confirmDelete(preview.id, phrase);
                  onStatus(status);
                  onDeleted(preview.scope);
                  setPreview(null);
                  setPhrase('');
                  setMessage('数据已按预览范围清理。');
                } catch (cause) {
                  setPreview(null);
                  setPhrase('');
                  setError(errorText(cause));
                } finally {
                  setBusy(false);
                }
              }}
            >
              确认{required}
            </button>
          </div>
        </div>
      )}
      <FormError message={error} />
      {message && <p role="status">{message}</p>}
    </section>
  );
}
