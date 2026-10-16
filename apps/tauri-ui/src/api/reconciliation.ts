import { invoke } from './client';
export interface ReconciliationRecord {
  id: string;
  account_id: string;
  cutoff: string;
  date: string;
  expected: string;
  actual: string;
  difference: string;
  adjustment_id: string | null;
  status: 'open' | 'reviewed' | 'closed';
  version: number;
  created_at: string;
  reference: string;
  reference_at: string;
  period_from: string | null;
  snapshot_ready: boolean;
}
export interface ReconciliationEntry {
  id: string;
  occurred_at: string;
  kind: string;
  status: 'pending' | 'confirmed' | 'ignored';
  delta: string;
  merchant: string | null;
  description: string | null;
  reviewed: boolean;
}
export interface ReconciliationSummary {
  record: ReconciliationRecord;
  entries: ReconciliationEntry[];
  pending_count: number;
  events: { action: string; occurred_at: string; reason: string | null }[];
}
export const reconciliationApi = {
  list: (accountId: string) => invoke<ReconciliationRecord[]>('reconciliation_list', { accountId }),
  summary: (id: string) => invoke<ReconciliationSummary>('reconciliation_summary', { id }),
  start: (accountId: string, date: string, actual: string) =>
    invoke<ReconciliationSummary>('reconciliation_start', { accountId, date, actual }),
  review: (r: ReconciliationRecord, entryId: string, reviewed: boolean) =>
    invoke<void>('reconciliation_review', { id: r.id, version: r.version, entryId, reviewed }),
  adjust: (r: ReconciliationRecord, reason: string) =>
    invoke<string>('reconciliation_adjust', { id: r.id, version: r.version, reason }),
  close: (r: ReconciliationRecord) =>
    invoke<void>('reconciliation_close', { id: r.id, version: r.version }),
  reopen: (r: ReconciliationRecord) =>
    invoke<void>('reconciliation_reopen', { id: r.id, version: r.version }),
};
