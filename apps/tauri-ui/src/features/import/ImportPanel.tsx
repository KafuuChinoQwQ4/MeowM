import { useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { importsApi, type BillPreview } from '../../api/imports';
import { useDialog } from '../../components/Dialog';
import { Field, FormError } from '../../components/Fields';
import { PlatformBadge } from '../../components/PlatformBadge';
import { errorText } from '../../shared/errors';
import { platformCategories } from '../../shared/labels';
import { ImportPreview } from './ImportPreview';
import { importSources, initialImportSource } from './importSources';

const formatNames = {
  wechat: '微信账单',
  alipay: '支付宝账单',
  bank: '银行卡账单',
} as const;

export function ImportPanel({
  accounts,
  accountId,
  onSaved,
}: {
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
}) {
  const sources = importSources(accounts);
  const platformSources = sources.filter((item) => item.accountId !== null);
  const formatSources = sources.filter((item) => item.accountId === null);
  const [sourceId, setSourceId] = useState(
    () => initialImportSource(sources, accounts, accountId).id,
  );
  const source =
    sources.find((item) => item.id === sourceId) ??
    initialImportSource(sources, accounts, accountId);
  const [preview, setPreview] = useState<BillPreview | null>(null);
  const [previewAccount, setPreviewAccount] = useState(() => source.accountId ?? accountId);
  const fileBytes = useRef<number[] | null>(null);
  const [needsPassword, setNeedsPassword] = useState(false);
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const { busy, setBusy } = useDialog();
  const running = useRef(false);

  async function load(bytes: number[], password?: string) {
    setError('');
    if (!source.provider) {
      setError('该平台暂不支持账单解析，请选择微信、支付宝或银行卡账单格式。');
      return;
    }
    try {
      const next = await importsApi.preview(source.provider, bytes, previewAccount, password);
      setPreview(next);
      fileBytes.current = null;
      setNeedsPassword(false);
    } catch (cause) {
      const message = errorText(cause);
      if (message.startsWith('PDF') && message.includes('密码')) setNeedsPassword(true);
      setError(message);
    } finally {
      setPassword('');
    }
  }

  async function choose(file?: File) {
    if (!file || running.current) return;
    if (file.size > 5 * 1024 * 1024) {
      setError('文件不能超过 5 MiB');
      return;
    }
    running.current = true;
    setBusy(true);
    setError('');
    try {
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      fileBytes.current = bytes;
      setNeedsPassword(false);
      setPassword('');
      await load(bytes);
    } catch (cause) {
      setError(errorText(cause));
    } finally {
      running.current = false;
      setBusy(false);
    }
  }

  if (preview)
    return (
      <ImportPreview
        preview={preview}
        accounts={accounts}
        accountId={previewAccount}
        onAccountChange={async (account) => {
          setBusy(true);
          try {
            const next = await importsApi.repreview(preview.preview_id, account);
            setPreviewAccount(account);
            setPreview(next);
          } finally {
            setBusy(false);
          }
        }}
        onSaved={onSaved}
      />
    );

  return (
    <div className="sheet-form import-panel">
      <div className="import-intro">
        <span className="import-intro-icon" aria-hidden="true">
          <svg viewBox="0 0 24 24">
            <path d="M7 3.75h7l4 4v12.5H7a2 2 0 0 1-2-2v-12.5a2 2 0 0 1 2-2Z" />
            <path d="M14 4v4h4M8.5 13h7M8.5 16.5h7" />
          </svg>
        </span>
        <div>
          <span className="import-eyebrow">本地账单导入</span>
          <p>账单先预览核对，确认后再加入待确认收件箱。</p>
        </div>
      </div>

      <section className="import-source-card" aria-label="账单来源设置">
        <div className="import-card-heading">
          <span className="import-step-number">01</span>
          <div>
            <strong>选择账单来源</strong>
            <small>平台名称和记账目标来自平台管理</small>
          </div>
        </div>
        <label className="form-field import-source-field">
          <span className="sr-only">来源</span>
          <select
            aria-label="来源"
            value={source.id}
            disabled={busy}
            onChange={(event) => {
              const next = sources.find((item) => item.id === event.target.value);
              if (!next) return;
              setSourceId(next.id);
              setPreviewAccount(next.accountId ?? accountId);
              fileBytes.current = null;
              setNeedsPassword(false);
              setPassword('');
              setError('');
            }}
          >
            {platformSources.length > 0 && (
              <optgroup label="平台管理中的平台">
                {platformSources.map((item) => (
                  <option key={item.id} value={item.id} disabled={!item.supported}>
                    {item.label}
                    {!item.supported
                      ? ` · ${platformCategories[item.category] ?? '其他平台'} · 暂不支持账单解析`
                      : ''}
                  </option>
                ))}
              </optgroup>
            )}
            {formatSources.length > 0 && (
              <optgroup label="未关联平台的账单格式">
                {formatSources.map((item) => (
                  <option key={item.id} value={item.id}>
                    {item.label}
                  </option>
                ))}
              </optgroup>
            )}
          </select>
        </label>
        <div className="import-source-summary" aria-live="polite">
          <PlatformBadge
            name={source.accountId ? source.label : '未关联平台'}
            category={source.category}
          />
          <div>
            <strong>{source.accountId ? '将记入所选平台' : '导入时再选择记账平台'}</strong>
            <small>
              {source.provider
                ? `按${formatNames[source.provider]}格式读取`
                : '该平台目前没有对应的账单解析格式'}
            </small>
          </div>
        </div>
      </section>

      <section className="import-file-section" aria-label="选择账单文件">
        <div className="import-card-heading">
          <span className="import-step-number">02</span>
          <div>
            <strong>选择账单文件</strong>
            <small>CSV、Excel 或银行流水 PDF，最多 5 MiB / 10,000 行</small>
          </div>
        </div>
        <label className="import-file-picker">
          <input
            aria-label="账单文件"
            type="file"
            accept=".csv,.xls,.xlsx,.pdf,text/csv,application/vnd.ms-excel,application/vnd.openxmlformats-officedocument.spreadsheetml.sheet,application/pdf"
            disabled={busy}
            onChange={(event) => void choose(event.target.files?.[0])}
          />
          <span className="import-file-icon" aria-hidden="true">
            <svg viewBox="0 0 24 24">
              <path d="M12 15.5V4m0 0L8 8m4-4 4 4M5 14.5v4a1.5 1.5 0 0 0 1.5 1.5h11a1.5 1.5 0 0 0 1.5-1.5v-4" />
            </svg>
          </span>
          <span className="import-file-copy">
            <strong>点击选择账单文件</strong>
            <small>支持 CSV · XLS · XLSX · PDF</small>
          </span>
          <span className="import-file-button">浏览文件</span>
        </label>
        <p className="import-format-note">
          支持 UTF-8 / GBK / GB18030 文本及中国银行交易流水明细 PDF。加密 PDF 会在本机提示输入密码；
          识别结果可在预览中核对，精确重复不会再次导入。
        </p>
      </section>

      {needsPassword && (
        <form
          className="import-password-card"
          onSubmit={(event) => {
            event.preventDefault();
            if (running.current || !fileBytes.current) return;
            const bytes = fileBytes.current;
            running.current = true;
            setBusy(true);
            void load(bytes, password).finally(() => {
              running.current = false;
              setBusy(false);
            });
          }}
        >
          <Field
            label="PDF 打开密码"
            type="password"
            value={password}
            disabled={busy}
            onChange={(event) => setPassword(event.target.value)}
          />
          <p className="field-hint">
            密码仅用于本次在本机读取 PDF，不会保存。读取成功后进入预览核对。
          </p>
          <button className="button button-primary" disabled={busy || !password}>
            解锁并读取账单
          </button>
        </form>
      )}
      <FormError message={error} />
    </div>
  );
}
