import type { RestorePreview } from '../../api/data';
const names: Record<string, string> = {
  accounts: '平台',
  categories: '分类',
  classification_rules: '分类规则',
  import_batches: '导入批次',
  transactions: '交易',
  postings: '分录',
  transaction_metadata: '交易版本与冲销关联',
  source_evidence: '来源摘要',
  budgets: '预算',
  recurring_templates: '周期模板',
  recurring_instances: '周期实例',
  notification_settings: '通知来源设置',
  app_settings: '金额显示偏好',
  audit_events: '审计事件',
  import_receipt_rows: '导入去重记录',
  notification_receipts: '通知去重记录',
  notification_status: '通知状态',
  merchant_aliases: '商户别名',
  reconciliation_sessions: '对账会话',
  reconciliation_entries: '对账明细快照',
  reconciliation_events: '对账审计',
  platform_metadata: '平台类型',
};
export function RestoreCounts({ preview }: { preview: RestorePreview }) {
  const current = new Map(preview.current_counts);
  return (
    <>
      <p>
        备份时间：{new Date(preview.backup.created_at).toLocaleString()} · 数据版本{' '}
        {preview.backup.schema_version}
      </p>
      <div className="data-counts">
        <table>
          <thead>
            <tr>
              <th>替换范围</th>
              <th>当前记录</th>
              <th>备份记录</th>
            </tr>
          </thead>
          <tbody>
            {preview.backup.counts.map(([name, count]) => (
              <tr key={name}>
                <th>{names[name] ?? name}</th>
                <td>{current.get(name) ?? 0}</td>
                <td>{count}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </>
  );
}
