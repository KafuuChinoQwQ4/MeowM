import { invoke } from './client';
import type { PendingEdit } from './entries';
export interface BillRow {
  id: string;
  row: number;
  date: string;
  amount: string;
  kind: string;
  merchant: string;
  description: string;
  decision: 'new' | 'exact_duplicate' | 'suspected_duplicate';
  duplicate_id: string | null;
  warnings: string[];
}
export interface BillPreview {
  preview_id: string;
  rows: BillRow[];
  errors: { row: number; message: string }[];
  new_count: number;
  exact_duplicates: number;
  suspected_duplicates: number;
  pending_count: number;
}
export const importsApi = {
  preview: (provider: string, bytes: number[]) =>
    invoke<BillPreview>('preview_bill', { provider, bytes }),
  stage: (previewId: string, accountId: string, ids: string[], edits: PendingEdit[]) =>
    invoke<string[]>('stage_bill', { previewId, accountId, ids, edits }),
};
