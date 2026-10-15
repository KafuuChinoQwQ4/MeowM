import { useCallback } from 'react';
import { recurringApi } from '../../api/recurring';
import { LoadState } from '../../components/LoadState';
import { money } from '../../shared/format';
import { useResource } from '../../shared/useResource';
const statuses = { pending: '待确认', confirmed: '已确认', ignored: '已忽略', skipped: '已跳过' };
export function RecurringHistory({ id }: { id: string }) {
  const state = useResource(useCallback(() => recurringApi.instances(id), [id]));
  return (
    <div className="sheet-form recurring-history">
      <p className="field-hint">最近 200 个实例；状态随待确认记录同步更新。</p>
      <LoadState {...state} />
      {state.data?.map((i) => (
        <article key={i.due_date}>
          <span>{i.due_date}</span>
          <strong>{statuses[i.status]}</strong>
          <span>{i.amount !== null ? `¥ ${money(i.amount)}` : '—'}</span>
        </article>
      ))}
      {state.data?.length === 0 && <p>尚无到期或跳过的实例。</p>}
    </div>
  );
}
