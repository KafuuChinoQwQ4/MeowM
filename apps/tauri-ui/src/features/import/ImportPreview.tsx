import { useEffect, useRef, useState } from 'react';
import type { Account } from '../../api/accounts';
import type { PendingEdit } from '../../api/entries';
import { importsApi, type BillPreview } from '../../api/imports';
import { useFormAction } from '../../components/Dialog';
import { SelectField, FormError } from '../../components/Fields';
import { validAmount, validDate } from '../../shared/format';
import { ImportRow } from './ImportRow';
import { defaultTransferMapping } from './transferMapping';
export function ImportPreview({
  preview,
  accounts,
  accountId,
  onSaved,
  onAccountChange,
}: {
  preview: BillPreview;
  accounts: Account[];
  accountId: string;
  onSaved: () => void;
  onAccountChange?: (account: string) => Promise<void>;
}) {
  type TransferField = 'account_id' | 'destination_id';
  const [account, setAccount] = useState(accountId);
  const [selected, setSelected] = useState(
    new Set(preview.rows.filter((row) => row.decision === 'new').map((row) => row.id)),
  );
  const [edits, setEdits] = useState<Record<string, PendingEdit>>({});
  const previous = useRef(preview);
  const formRef = useRef<HTMLFormElement>(null);
  const [attention, setAttention] = useState<{ rowId: string; field: TransferField } | null>(null);
  useEffect(() => {
    const before = previous.current;
    if (before === preview) return;
    const oldRows = new Map(before.rows.map((row) => [row.row, row]));
    setAccount(accountId);
    setSelected(
      (old) =>
        new Set(
          preview.rows
            .filter((row) => {
              const prior = oldRows.get(row.row);
              if (row.decision === 'exact_duplicate') return false;
              if (!prior) return row.decision === 'new';
              return (
                old.has(prior.id) &&
                (row.decision !== 'suspected_duplicate' || prior.decision === 'suspected_duplicate')
              );
            })
            .map((row) => row.id),
        ),
    );
    setEdits((old) =>
      Object.fromEntries(
        preview.rows.flatMap((row) => {
          const prior = oldRows.get(row.row);
          const edit = prior && old[prior.id];
          return edit ? [[row.id, { ...edit, id: row.id }]] : [];
        }),
      ),
    );
    setAttention(null);
    previous.current = preview;
  }, [preview, accountId]);
  const action = useFormAction(onSaved);
  useEffect(() => {
    if (!attention) return;
    const card = Array.from(
      formRef.current?.querySelectorAll<HTMLElement>('[data-import-row-id]') ?? [],
    ).find((element) => element.dataset.importRowId === attention.rowId);
    const field = card?.querySelector<HTMLSelectElement>(
      `[data-transfer-field="${attention.field}"]`,
    );
    card?.scrollIntoView({ behavior: 'auto', block: 'center' });
    field?.focus({ preventScroll: true });
  }, [attention]);
  function submit() {
    if (!selected.size) {
      action.setError('请选择至少一条有效候选记录');
      return;
    }
    const selectedEdits = preview.rows
      .filter((row) => selected.has(row.id))
      .flatMap((row) => {
        const edit = edits[row.id];
        if (row.kind !== 'transfer') {
          return edit ? [{ ...edit, account_id: account }] : [];
        }
        const mapping = defaultTransferMapping(row, accounts, account);
        const candidate: PendingEdit = Object.assign(
          {
            id: row.id,
            expected_version: 0,
            date: row.date,
            amount: row.amount,
            kind: row.kind,
            merchant: row.merchant,
            description: row.description,
            category_id: null,
            original_transaction_id: null,
            ...mapping,
          },
          edit,
        );
        candidate.account_id = edit?.account_id ?? mapping.account_id;
        candidate.destination_id = edit?.destination_id ?? mapping.destination_id;
        return [candidate];
      });
    const incompleteTransfer = selectedEdits.find(
      (edit) => edit.kind === 'transfer' && (!edit.account_id || !edit.destination_id),
    );
    if (incompleteTransfer) {
      setAttention({
        rowId: incompleteTransfer.id,
        field: incompleteTransfer.account_id ? 'destination_id' : 'account_id',
      });
      action.setError('转账记录需选择转出、转入平台；同平台内部划转可选择相同平台。');
      return;
    }
    setAttention(null);
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
      ref={formRef}
      className="sheet-form import-preview-form"
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
          onChange={(e) => {
            const next = e.target.value;
            if (onAccountChange)
              void onAccountChange(next).catch((error: unknown) => action.setError(String(error)));
            else setAccount(next);
          }}
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
            accounts={accounts}
            accountId={account}
            selected={selected.has(row.id)}
            edit={edits[row.id]}
            attentionField={attention?.rowId === row.id ? attention.field : null}
            onEdit={(edit) => {
              setEdits((old) => ({ ...old, [row.id]: edit }));
              setAttention(null);
              action.setError('');
            }}
            onSelect={(checked) => {
              setSelected((old) => {
                const next = new Set(old);
                if (checked) next.add(row.id);
                else next.delete(row.id);
                return next;
              });
              setAttention(null);
              action.setError('');
            }}
          />
        ))}
      </fieldset>
      <div className="import-preview-actions">
        <FormError message={action.error} />
        <button className="button button-primary submit-button" disabled={action.busy || !account}>
          加入待确认收件箱
        </button>
      </div>
    </form>
  );
}
