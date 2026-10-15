import { useState } from 'react';
import type { Account } from '../../api/accounts';
import type { PendingEdit } from '../../api/entries';
import { importsApi, type BillPreview } from '../../api/imports';
import { useFormAction } from '../../components/Dialog';
import { SelectField, FormError } from '../../components/Fields';
import { validAmount, validDate } from '../../shared/format';
import { ImportRow } from './ImportRow';
export function ImportPreview({
  preview,
  accounts,
  accountId,
  onSaved,
}: {
  preview: BillPreview;
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
}) {
  const [account, setAccount] = useState(accountId);
  const [selected, setSelected] = useState(
    new Set(preview.rows.filter((row) => row.decision === 'new').map((row) => row.id)),
  );
  const [edits, setEdits] = useState<Record<string, PendingEdit>>({});
  const action = useFormAction(onSaved);
  function submit() {
    if (!selected.size) {
      action.setError('请选择至少一条有效候选记录');
      return;
    }
    const selectedEdits = Object.values(edits)
      .filter((edit) => selected.has(edit.id))
      .map((edit) => ({ ...edit, account_id: account }));
    if (selectedEdits.some((edit) => !validAmount(edit.amount) || !validDate(edit.date))) {
      action.setError('请输入有效正金额和日期');
      return;
    }
    void action.run(() =>
      importsApi.stage(preview.preview_id, account, [...selected], selectedEdits),
    );
  }
  return (
    <form
      className="sheet-form"
      onSubmit={(e) => {
        e.preventDefault();
        submit();
      }}
    >
      <p className="sheet-description">
        新增 {preview.new_count} · 精确重复 {preview.exact_duplicates} · 疑似重复{' '}
        {preview.suspected_duplicates} · 可待确认 {preview.pending_count}
      </p>
      <fieldset disabled={action.busy}>
        <SelectField
          label="记入平台"
          options={accounts.map((a) => [a.id, a.name])}
          value={account}
          onChange={(e) => setAccount(e.target.value)}
        />
        {preview.errors.map((error, i) => (
          <p key={i} className="form-error">
            第 {error.row} 行：{error.message}
          </p>
        ))}
        {preview.rows.map((row) => (
          <ImportRow
            key={row.id}
            row={row}
            selected={selected.has(row.id)}
            edit={edits[row.id]}
            onEdit={(edit) => setEdits((old) => ({ ...old, [row.id]: edit }))}
            onSelect={(checked) =>
              setSelected((old) => {
                const next = new Set(old);
                if (checked) next.add(row.id);
                else next.delete(row.id);
                return next;
              })
            }
          />
        ))}
      </fieldset>
      <FormError message={action.error} />
      <button className="button button-primary submit-button" disabled={action.busy || !account}>
        加入待确认收件箱
      </button>
    </form>
  );
}
