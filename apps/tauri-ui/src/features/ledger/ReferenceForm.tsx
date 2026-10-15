import { useState } from 'react';
import type { DayView } from '../../api/ledger';
import { ledgerApi } from '../../api/ledger';
import { useDialog, useFormAction } from '../../components/Dialog';
import { Field, FormError } from '../../components/Fields';
import { validAmount } from '../../shared/format';
export function ReferenceForm({
  accountId,
  date,
  day,
  clear,
  onSaved,
}: {
  accountId: string;
  date: string;
  day: DayView;
  clear: boolean;
  onSaved: () => void;
}) {
  const [amount, setAmount] = useState(day.reference?.date === date ? day.reference.amount : '');
  const action = useFormAction(onSaved);
  const { close } = useDialog();
  return (
    <form
      className="sheet-form"
      onSubmit={(e) => {
        e.preventDefault();
        if (!clear && !validAmount(amount.trim(), true)) {
          action.setError('请输入有效余额，最多保留两位小数。');
          return;
        }
        void action.run(() =>
          clear
            ? ledgerApi.clearReference(accountId)
            : ledgerApi.setReference(accountId, date, amount.trim()),
        );
      }}
    >
      <p className="sheet-description">
        {clear
          ? '清除后，此平台将不再显示日末余额。已有的收入和支出记录会保留，之后可以重新设置。'
          : '输入这一天结束时的实际余额。我们会结合收支记录，推算其他日期的日末余额。'}
      </p>
      <fieldset disabled={action.busy}>
        {clear ? (
          <button type="button" className="button button-secondary" onClick={close}>
            保留参考余额
          </button>
        ) : (
          <>
            <Field
              label="当日日末余额（元）"
              required
              autoFocus
              value={amount}
              onChange={(e) => setAmount(e.target.value)}
              placeholder="0.00"
              className="amount-input"
            />
            <p className="field-hint">可以是零或负数；新设置会替换此平台原有的参考余额。</p>
          </>
        )}
      </fieldset>
      <FormError message={action.error} />
      <button className="button button-primary submit-button" disabled={action.busy}>
        {action.busy ? '正在保存…' : clear ? '确认清除' : '保存参考余额'}
      </button>
    </form>
  );
}
