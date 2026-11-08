import type { PrivacyStatus } from '../../api/privacy';
import { LockSettings } from './LockSettings';
import { DeleteData } from './DeleteData';
export function PrivacyPage({
  status,
  onStatus,
  onDeleted,
  onNavigate,
}: {
  status: PrivacyStatus;
  onStatus: (v: PrivacyStatus) => void;
  onDeleted: (scope: 'notifications' | 'all') => void;
  onNavigate: (view: 'notifications' | 'data') => void;
}) {
  return (
    <section className="data-page" aria-label="隐私与数据">
      <h2>隐私与数据</h2>
      <LockSettings status={status} onStatus={onStatus} />
      <section className="data-card">
        <h3>通知来源</h3>
        <button className="button" onClick={() => onNavigate('notifications')}>
          管理通知来源开关
        </button>
      </section>
      <DeleteData onStatus={onStatus} onDeleted={onDeleted} onBackup={() => onNavigate('data')} />
    </section>
  );
}
