import { accountsApi, type Account } from '../../api/accounts';
import { useDialog, useFormAction } from '../../components/Dialog';
import { FormError } from '../../components/Fields';

export function DeleteAccountForm({
  account,
  onDeleted,
}: {
  account: Account;
  onDeleted: () => void;
}) {
  const { close } = useDialog();
  const action = useFormAction(onDeleted);
  return (
    <form
      className="sheet-form platform-delete-form"
      onSubmit={(event) => {
        event.preventDefault();
        void action.run(() => accountsApi.delete(account.id));
      }}
    >
      <p>确定删除平台「{account.name}」？删除后无法恢复。</p>
      <p className="field-hint">
        有关联流水、预算、周期账单、对账记录或采集设置的平台无法删除，请先处理关联。
      </p>
      <FormError message={action.error} />
      <div className="platform-delete-actions">
        <button
          type="button"
          className="button button-secondary"
          disabled={action.busy}
          onClick={close}
        >
          取消
        </button>
        <button type="submit" className="button button-danger" disabled={action.busy}>
          {action.busy ? '正在删除…' : '确认删除'}
        </button>
      </div>
    </form>
  );
}
