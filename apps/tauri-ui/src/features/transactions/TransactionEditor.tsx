import { useState } from 'react';
import type { Account } from '../../api/accounts';
import type { CategoryOption } from '../../api/entries';
import {
  transactionsApi,
  type TransactionDetail,
  type TransactionWrite,
} from '../../api/transactions';
import { useFormAction } from '../../components/Dialog';
import { Field, SelectField, FormError } from '../../components/Fields';
import { editableKinds, kindNames } from '../../shared/labels';
import { money, validAmount, validDate, today } from '../../shared/format';
import { impact } from './impact';
export function TransactionEditor({
  detail,
  accounts,
  categories,
  accountId,
  date,
  onSaved,
}: {
  detail: TransactionDetail | null;
  accounts: Account[];
  categories: CategoryOption[];
  accountId: string;
  date: string;
  onSaved: () => void;
}) {
  const tx = detail?.transaction;
  const [draft, setDraft] = useState<TransactionWrite>({
    id: tx?.id,
    expected_version: detail?.edit_version ?? 0,
    kind: tx?.kind ?? 'expense',
    amount: tx?.amount ?? '',
    date: tx?.date ?? date,
    time: tx?.time ?? (date === today() ? new Date().toTimeString().slice(0, 8) : ''),
    source: '',
    transaction_number: '',
    merchant_order_id: '',
    account_id: tx?.account_id ?? accountId,
    to_account_id: tx?.to_account_id ?? '',
    category_id: tx?.category_id ?? '',
    merchant: tx?.merchant ?? '',
    description: tx?.description ?? '',
    original_transaction_id: tx?.original_transaction_id ?? '',
  });
  const [snapshot, setSnapshot] = useState('');
  const [preview, setPreview] = useState('');
  const [confirming, setConfirming] = useState(false);
  const action = useFormAction(onSaved);
  function update(key: keyof TransactionWrite, value: string) {
    setDraft((d) => ({ ...d, [key]: value }));
  }
  function showImpact() {
    action.setError('');
    if (!validAmount(draft.amount) || !validDate(draft.date)) {
      action.setError('请输入有效的正金额和日期。');
      return;
    }
    if (!tx && draft.kind === 'expense' && !draft.merchant?.trim()) {
      action.setError('请填写实际消费商户，便于以后与账单核对。');
      return;
    }
    if ((draft.transaction_number?.trim() || draft.merchant_order_id?.trim()) && !draft.source) {
      action.setError('填写单号时请选择支付渠道。');
      return;
    }
    if (draft.kind === 'transfer' && !draft.to_account_id) {
      action.setError('请选择转入账户；同平台内部划转可选择相同账户。');
      return;
    }
    const internalTransfer = draft.kind === 'transfer' && draft.to_account_id === draft.account_id;
    setPreview(
      internalTransfer && (!tx || tx.account_id === tx.to_account_id)
        ? `保存后保留一笔平台内部划转，平台总余额和收支不变。${tx && draft.account_id !== tx.account_id ? '记录归属的平台会调整。' : ''}`
        : tx
          ? impact(
              draft.kind,
              draft.amount,
              tx.amount,
              draft.account_id !== tx.account_id ||
                draft.to_account_id !== (tx.to_account_id ?? ''),
              draft.date !== tx.date,
            )
          : `保存后将新增一笔${kindNames[draft.kind]} ¥ ${money(draft.amount)}。${draft.kind === 'transfer' ? '转账只改变两端余额。' : draft.kind === 'refund' ? '退款会抵减净支出。' : ''}`,
    );
    setSnapshot(JSON.stringify(draft));
    setConfirming(true);
  }
  function save() {
    if (snapshot !== JSON.stringify(draft)) {
      action.setError('内容已修改，请重新查看影响。');
      setConfirming(false);
      return;
    }
    void action.run(() =>
      transactionsApi.save({
        ...draft,
        merchant: draft.merchant?.trim(),
        description: draft.description?.trim(),
        category_id: draft.category_id || null,
        to_account_id: draft.kind === 'transfer' ? draft.to_account_id : null,
        original_transaction_id:
          draft.kind === 'refund' ? draft.original_transaction_id?.trim() || null : null,
      }),
    );
  }
  return (
    <form onSubmit={(e) => e.preventDefault()}>
      {preview && <p className="impact-preview">{preview}</p>}
      <fieldset disabled={action.busy}>
        {!tx && (
          <p className="field-hint">
            账户选实际记账平台，商户填实际消费店名，日期和时间按交易发生时填写。备注可选；有渠道交易单号时优先据此去重，商户单号辅助核对。没有单号时只提示疑似重复，不会自动合并。
          </p>
        )}
        <SelectField
          label="类型"
          options={editableKinds}
          value={draft.kind}
          disabled={!!tx}
          onChange={(e) => update('kind', e.target.value)}
        />
        <SelectField
          label="账户"
          options={accounts.map((a) => [a.id, a.name])}
          value={draft.account_id}
          onChange={(e) => update('account_id', e.target.value)}
        />
        {draft.kind === 'transfer' && (
          <>
            <SelectField
              label="转入账户（转账时必填）"
              options={[
                ['', '请选择账户'],
                ...accounts.map((a) => [a.id, a.name] as [string, string]),
              ]}
              value={draft.to_account_id ?? ''}
              onChange={(e) => update('to_account_id', e.target.value)}
            />
            <p className="field-hint">
              花呗、余额和余额宝可统一记在支付宝；同平台内部划转可选择相同账户，平台总余额和收支不变。
            </p>
          </>
        )}
        <Field
          label="日期"
          type="date"
          value={draft.date}
          onChange={(e) => update('date', e.target.value)}
        />
        <Field
          label="金额（元）"
          inputMode="decimal"
          required
          value={draft.amount}
          onChange={(e) => update('amount', e.target.value)}
        />
        <Field
          label="交易时间（可选）"
          type="time"
          step="1"
          value={draft.time ?? ''}
          onChange={(e) => update('time', e.target.value)}
        />
        <SelectField
          label="分类"
          options={[['', '未分类'], ...categories.map((c) => [c.id, c.name] as [string, string])]}
          value={draft.category_id ?? ''}
          onChange={(e) => update('category_id', e.target.value)}
        />
        <Field
          label="商户"
          maxLength={100}
          value={draft.merchant}
          onChange={(e) => update('merchant', e.target.value)}
        />
        <Field
          label="备注"
          maxLength={200}
          value={draft.description}
          onChange={(e) => update('description', e.target.value)}
        />
        {!tx && (
          <details>
            <summary>交易单号（可选，用于去重）</summary>
            <SelectField
              label="支付渠道"
              value={draft.source ?? ''}
              options={[
                ['', '请选择支付渠道'],
                ['wechat', '微信'],
                ['alipay', '支付宝'],
                ['bank', '银行卡'],
              ]}
              onChange={(e) => update('source', e.target.value)}
            />
            <Field
              label="渠道交易单号"
              maxLength={200}
              value={draft.transaction_number ?? ''}
              onChange={(e) => update('transaction_number', e.target.value)}
            />
            <Field
              label="商户单号"
              maxLength={200}
              value={draft.merchant_order_id ?? ''}
              onChange={(e) => update('merchant_order_id', e.target.value)}
            />
            <p className="field-hint">
              可从支付详情复制，保留前导
              0。渠道交易单号与商户单号应分别填写；备注无需复制账单商品说明。
            </p>
          </details>
        )}
        {draft.kind === 'refund' && (
          <Field
            label="退款原消费 ID（可选）"
            disabled={!!tx}
            value={draft.original_transaction_id ?? ''}
            onChange={(e) => update('original_transaction_id', e.target.value)}
          />
        )}
      </fieldset>
      <FormError message={action.error} />
      {confirming ? (
        <button
          type="button"
          disabled={action.busy}
          className="button button-primary transaction-submit"
          onClick={save}
        >
          确认保存修改
        </button>
      ) : (
        <button
          type="button"
          className="button button-secondary transaction-submit"
          onClick={showImpact}
        >
          查看影响并保存
        </button>
      )}
    </form>
  );
}
