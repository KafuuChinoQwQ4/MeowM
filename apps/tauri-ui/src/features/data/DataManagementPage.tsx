import { useState } from 'react';
import type { Account } from '../../api/accounts';
import { BackupForm } from './BackupForm';
import { RestoreForm } from './RestoreForm';
import { ExportForm } from './ExportForm';
export function DataManagementPage({
  accounts,
  onRestored,
}: {
  accounts: Account[];
  onRestored: () => void;
}) {
  const [busy, setBusy] = useState(false);
  return (
    <section className="data-page" aria-label="备份与导出" aria-busy={busy}>
      <h2>备份与导出</h2>
      {busy && <p role="status">正在处理，请稍候…</p>}
      <BackupForm busy={busy} onBusy={setBusy} />
      <RestoreForm busy={busy} onBusy={setBusy} onRestored={onRestored} />
      <ExportForm accounts={accounts} busy={busy} onBusy={setBusy} />
    </section>
  );
}
