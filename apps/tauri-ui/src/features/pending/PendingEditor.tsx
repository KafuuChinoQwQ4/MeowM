import { useCallback, useState } from 'react';
import type { Account } from '../../api/accounts';
import { entriesApi, type PageEntry, type PendingEdit } from '../../api/entries';
import { useResource } from '../../shared/useResource';
import { useFormAction } from '../../components/Dialog';
import { Field, SelectField, FormError } from '../../components/Fields';
import { kindNames } from '../../shared/labels';
import { validDate, validAmount } from '../../shared/format';
export function PendingEditor({
  entry,
  accounts,
  accountId,
  onSaved,
}: {
  entry: PageEntry;
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
}) {
  const occurred = new Date(entry.occurred_at);
  const [draft, setDraft] = useState<PendingEdit>({
    id: entry.id,
    expected_version: entry.edit_version,
    account_id: entry.account_id ?? accountId,
    date: `${occurred.getFullYear()}-${String(occurred.getMonth() + 1).padStart(2, '0')}-${String(occurred.getDate()).padStart(2, '0')}`,
    amount: entry.amount ?? '',
    kind: entry.kind,
    destination_id: entry.destination_id ?? null,
    merchant: entry.merchant ?? '',
    description: entry.description ?? '',
    category_id: entry.category_id,
    original_transaction_id: entry.original_transaction_id,
  });
  const categories = useResource(useCallback(() => entriesApi.categories(), []));
  const action = useFormAction(onSaved);
  function update(key: keyof PendingEdit, value: string) {
    setDraft((d) => ({ ...d, [key]: value }));
  }
  function save(confirm: boolean) {
    if (!validAmount(draft.amount.trim()) || !validDate(draft.date)) {
      action.setError('请输入有效正金额和日期');
      return;
    }
    const edit = {
      ...draft,
      amount: draft.amount.trim(),
      merchant: draft.merchant.trim(),
      description: draft.description.trim(),
      category_id: categories.data ? draft.category_id || null : entry.category_id,
      original_transaction_id: draft.original_transaction_id?.trim() || null,
    };
    void action.run(() =>
      confirm ? entriesApi.confirm([entry.id], [edit]) : entriesApi.edit(edit),
    );
  }
  const options: [string, string][] = [
    ['', '未分类'],
    ...(categories.data ?? []).map((c) => [c.id, c.name] as [string, string]),
  ];
  if (draft.category_id && !options.some(([id]) => id === draft.category_id))
    options.push([draft.category_id, '当前分类']);
  return (
    <form
      className="sheet-form"
      onSubmit={(e) => {
        e.preventDefault();
        save(true);
      }}
    >
      <fieldset disabled={action.busy}>
        <SelectField
          label="记入平台"
          options={accounts.map((a) => [a.id, a.name])}
          value={draft.account_id}
          onChange={(e) => update('account_id', e.target.value)}
        />
        <Field
          label="日期"
          type="date"
          value={draft.date}
          onChange={(e) => update('date', e.target.value)}
        />
        <Field
          label="金额（元）"
          value={draft.amount}
          onChange={(e) => update('amount', e.target.value)}
        />
        <SelectField
          label="类型"
          options={Object.entries(kindNames).filter(([id]) =>
            ['expense', 'income', 'refund', 'transfer'].includes(id),
          )}
          value={draft.kind}
          onChange={(e) => update('kind', e.target.value)}
        />
        {draft.kind === 'transfer' && (
          <SelectField
            label="转入平台"
            value={draft.destination_id ?? ''}
            options={[
              ['', '请选择转入平台'],
              ...accounts
                .filter((a) => a.id !== draft.account_id)
                .map((a) => [a.id, a.name] as [string, string]),
            ]}
            onChange={(e) => update('destination_id', e.target.value)}
          />
        )}
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
        <SelectField
          label="分类"
          options={options}
          disabled={!categories.data}
          value={draft.category_id ?? ''}
          onChange={(e) => update('category_id', e.target.value)}
        />
        {categories.error && (
          <>
            <FormError message={categories.error} />
            <button type="button" className="button button-secondary" onClick={categories.reload}>
              重试分类
            </button>
          </>
        )}
        <Field
          label="退款原消费 ID（退款必填）"
          value={draft.original_transaction_id ?? ''}
          onChange={(e) => update('original_transaction_id', e.target.value)}
        />
        <button type="button" className="button button-secondary" onClick={() => save(false)}>
          仅保存修改
        </button>
      </fieldset>
      <FormError message={action.error} />
      <button className="button button-primary submit-button" disabled={action.busy}>
        保存并确认
      </button>
    </form>
  );
}
