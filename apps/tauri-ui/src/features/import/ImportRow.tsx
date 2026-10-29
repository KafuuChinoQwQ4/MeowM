import type { BillRow } from '../../api/imports';
import type { PendingEdit } from '../../api/entries';
import { Field } from '../../components/Fields';
import { SelectField } from '../../components/Fields';
import type { Account } from '../../api/accounts';
import { kindNames } from '../../shared/labels';
import { money } from '../../shared/format';
import { defaultTransferMapping } from './transferMapping';
export function ImportRow({
  row,
  accounts,
  accountId,
  selected,
  edit,
  attentionField,
  onSelect,
  onEdit,
}: {
  row: BillRow;
  accounts: Account[];
  accountId: string;
  selected: boolean;
  edit?: PendingEdit;
  attentionField: 'account_id' | 'destination_id' | null;
  onSelect: (selected: boolean) => void;
  onEdit: (edit: PendingEdit) => void;
}) {
  const mapping = defaultTransferMapping(row, accounts, accountId);
  function change(changes: Partial<PendingEdit>) {
    onEdit({
      ...{
        id: row.id,
        expected_version: 0,
        account_id: mapping.account_id,
        destination_id: mapping.destination_id,
        date: row.date,
        amount: row.amount,
        kind: row.kind,
        merchant: row.merchant,
        description: row.description,
        category_id: null,
        original_transaction_id: null,
      },
      ...edit,
      ...changes,
    });
  }
  return (
    <article
      className={`pending-card${attentionField ? ' import-row-needs-attention' : ''}`}
      data-import-row-id={row.id}
    >
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
      {row.transaction_number && (
        <p className="field-hint">渠道交易单号：{row.transaction_number}</p>
      )}
      {row.merchant_order_id && <p className="field-hint">商户单号：{row.merchant_order_id}</p>}
      {row.account_hint && <p className="field-hint">原文件账户：{row.account_hint}</p>}
      {row.kind === 'transfer' && row.decision !== 'exact_duplicate' && (
        <>
          <p className="field-hint">
            {row.transfer_direction === 'outgoing'
              ? '账单金额显示资金从记入平台转出。'
              : row.transfer_direction === 'incoming'
                ? '账单金额显示资金转入记入平台。'
                : '账单没有明确转入或转出方向。'}
            {` 对方平台按账单中的对方名称匹配；未匹配时请补选。`}{' '}
            同平台内部划转可选择相同平台，不计收入或支出。
          </p>
          <button
            type="button"
            className="button button-secondary"
            disabled={!accountId}
            onClick={() => change({ account_id: accountId, destination_id: accountId })}
          >
            记为当前平台内部划转
          </button>
          {edit?.account_id && edit.account_id === edit.destination_id && (
            <p className="field-hint">平台内部划转：保留记录，平台总余额和收支不变。</p>
          )}
          <SelectField
            label={`第 ${row.row} 行转出平台`}
            data-transfer-field="account_id"
            value={edit?.account_id ?? mapping.account_id}
            options={[
              ['', '请选择转出平台'],
              ...accounts.map((a) => [a.id, a.name] as [string, string]),
            ]}
            onChange={(e) => change({ account_id: e.target.value })}
          />
          <SelectField
            label={`第 ${row.row} 行转入平台`}
            data-transfer-field="destination_id"
            value={edit?.destination_id ?? mapping.destination_id}
            options={[
              ['', '请选择转入平台'],
              ...accounts.map((a) => [a.id, a.name] as [string, string]),
            ]}
            onChange={(e) => change({ destination_id: e.target.value })}
          />
        </>
      )}
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
            onChange={(e) => change({ date: e.target.value })}
          />
          <Field
            label="金额（元）"
            value={edit?.amount ?? row.amount}
            onChange={(e) => change({ amount: e.target.value })}
          />
          <Field
            label="商户"
            maxLength={100}
            value={edit?.merchant ?? row.merchant}
            onChange={(e) => change({ merchant: e.target.value })}
          />
          <Field
            label="备注"
            maxLength={200}
            value={edit?.description ?? row.description}
            onChange={(e) => change({ description: e.target.value })}
          />
        </details>
      )}
    </article>
  );
}
