import { invoke } from './client';
import type { PendingEdit } from './entries';
export interface BillRow {
  id: string;
  row: number;
  date: string;
  amount: string;
  kind: string;
  transfer_direction?: 'outgoing' | 'incoming' | null;
  merchant: string;
  description: string;
  decision: 'new' | 'exact_duplicate' | 'suspected_duplicate';
  duplicate_id: string | null;
  warnings: string[];
  transaction_number?: string | null;
  merchant_order_id?: string | null;
  account_hint?: string | null;
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
  preview: (provider: string, bytes: number[], accountId?: string, password?: string) =>
    invoke<BillPreview>('preview_bill', {
      provider,
      bytes,
      accountId: accountId || null,
      password: password ?? null,
    }),
  repreview: (previewId: string, accountId: string) =>
    invoke<BillPreview>('preview_bill', { provider: '', bytes: [], previewId, accountId }),
  stage: (previewId: string, accountId: string, ids: string[], edits: PendingEdit[]) =>
    invoke<string[]>('stage_bill', { previewId, accountId, ids, edits }),
};
