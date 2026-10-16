import { invoke } from './client';
export interface PageQuery {
  status?: 'pending' | 'confirmed' | 'ignored';
  source?: 'wechat' | 'alipay' | 'bank' | 'notification' | 'manual' | 'recurring';
  account_id?: string;
  category_id?: string;
  from_date?: string;
  until_date?: string;
  merchant?: string;
  keyword?: string;
  min_amount?: string;
  max_amount?: string;
  offset: number;
  limit: number;
}
export interface PageEntry {
  id: string;
  status: 'pending' | 'confirmed' | 'ignored';
  kind: string;
  occurred_at: string;
  amount: string | null;
  merchant: string | null;
  description: string | null;
  edit_version: number;
  reversal_of: string | null;
  account_id: string | null;
  destination_id?: string | null;
  category_id: string | null;
  original_transaction_id: string | null;
  sources: string[];
  suggestion?: { category_id: string; rule_id: string; reason: string } | null;
}
export interface EntryPage {
  items: PageEntry[];
  total: number;
  offset: number;
  limit: number;
}
export interface CommandError {
  code: 'invalid_input' | 'not_found' | 'conflict' | 'storage_error' | 'busy' | 'locked';
  message: string;
}
export interface PendingEdit {
  id: string;
  expected_version: number;
  destination_id?: string | null;
  account_id: string;
  date: string;
  amount: string;
  kind: string;
  merchant: string;
  description: string;
  category_id: string | null;
  original_transaction_id: string | null;
}

export interface CategoryOption {
  id: string;
  name: string;
}
export const entriesApi = {
  query: (query: PageQuery) => invoke<EntryPage>('query_entries', { query }),
  confirm: (ids: string[], edits: PendingEdit[] = []) =>
    invoke<void>('confirm_pending', { ids, edits }),
  edit: (edit: PendingEdit) => invoke<void>('edit_pending', { edit }),
  ignore: (ids: string[]) => invoke<void>('ignore_pending', { ids }),
  restore: (id: string) => invoke<void>('restore_pending', { id }),
  categories: () => invoke<CategoryOption[]>('list_categories'),
};
