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
        <p>关闭微信或支付宝来源后，停止处理该来源的新通知；已有候选和已确认交易仍保留。</p>
        <button className="button" onClick={() => onNavigate('notifications')}>
          管理通知来源开关
        </button>
      </section>
      <DeleteData onStatus={onStatus} onDeleted={onDeleted} onBackup={() => onNavigate('data')} />
    </section>
  );
}
