import type { BillRow } from '../../api/imports';
import type { PendingEdit } from '../../api/entries';
import { Field } from '../../components/Fields';
import { kindNames } from '../../shared/labels';
import { money } from '../../shared/format';
export function ImportRow({
  row,
  selected,
  edit,
  onSelect,
  onEdit,
}: {
  row: BillRow;
  selected: boolean;
  edit?: PendingEdit;
  onSelect: (selected: boolean) => void;
  onEdit: (edit: PendingEdit) => void;
}) {
  function change(key: keyof PendingEdit, value: string) {
    onEdit({
      ...{
        id: row.id,
        expected_version: 0,
        account_id: '',
        date: row.date,
        amount: row.amount,
        kind: row.kind,
        merchant: row.merchant,
        description: row.description,
        category_id: null,
        original_transaction_id: null,
      },
      ...edit,
      [key]: value,
    });
  }
  return (
    <article className="pending-card">
      <label className="pending-top">
        <input
          type="checkbox"
          checked={selected}
          disabled={row.decision === 'exact_duplicate'}
          onChange={(e) => onSelect(e.target.checked)}
        />
        <strong>
          第 {row.row} 行 · {row.merchant || row.description || kindNames[row.kind]}
        </strong>
        <span>¥{money(row.amount)}</span>
      </label>
      <p className="field-hint">
        {row.date} · {kindNames[row.kind]} ·{' '}
        {row.decision === 'new'
          ? '新增'
          : row.decision === 'exact_duplicate'
            ? '精确重复（不导入）'
            : '疑似重复（请核对后勾选）'}
      </p>
      {row.duplicate_id && <p className="field-hint">已存交易：{row.duplicate_id}</p>}
      {row.warnings.map((text, i) => (
        <p key={i} className="field-hint">
          {text}
        </p>
      ))}
      {row.decision !== 'exact_duplicate' && (
        <details>
          <summary>修订金额、日期、商户或备注</summary>
          <Field
            label="日期"
            type="date"
            value={edit?.date ?? row.date}
            onChange={(e) => change('date', e.target.value)}
          />
          <Field
            label="金额（元）"
            value={edit?.amount ?? row.amount}
            onChange={(e) => change('amount', e.target.value)}
          />
          <Field
            label="商户"
            maxLength={100}
            value={edit?.merchant ?? row.merchant}
            onChange={(e) => change('merchant', e.target.value)}
          />
          <Field
            label="备注"
            maxLength={200}
            value={edit?.description ?? row.description}
            onChange={(e) => change('description', e.target.value)}
          />
        </details>
      )}
    </article>
  );
}
