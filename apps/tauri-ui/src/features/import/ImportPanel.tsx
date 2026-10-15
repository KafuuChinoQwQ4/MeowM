import { useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import { importsApi, type BillPreview } from '../../api/imports';
import { useDialog } from '../../components/Dialog';
import { SelectField, FormError } from '../../components/Fields';
import { errorText } from '../../shared/errors';
import { ImportPreview } from './ImportPreview';
export function ImportPanel({
  accounts,
  accountId,
  onSaved,
}: {
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
}) {
  const [provider, setProvider] = useState('wechat');
  const [preview, setPreview] = useState<BillPreview | null>(null);
  const [error, setError] = useState('');
  const { busy, setBusy } = useDialog();
  const running = useRef(false);
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
      setPreview(
        await importsApi.preview(provider, Array.from(new Uint8Array(await file.arrayBuffer()))),
      );
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
        accountId={accountId}
        onSaved={onSaved}
      />
    );
  return (
    <div className="sheet-form">
      <p className="sheet-description">
        选择 UTF-8 CSV
        文件。有效候选先进入待确认，精确重复不再次导入。支持微信、支付宝和银行的日期/金额表头。
      </p>
      <SelectField
        label="来源"
        options={[
          ['wechat', '微信'],
          ['alipay', '支付宝'],
          ['bank', '银行卡'],
        ]}
        value={provider}
        disabled={busy}
        onChange={(e) => setProvider(e.target.value)}
      />
      <label className="form-field">
        账单 CSV（最多 5 MiB / 1000 行）
        <input
          type="file"
          accept=".csv,text/csv"
          disabled={busy}
          onChange={(e) => void choose(e.target.files?.[0])}
        />
      </label>
      <FormError message={error} />
    </div>
  );
}
