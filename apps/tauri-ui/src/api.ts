import { invoke } from '@tauri-apps/api/core';
export interface Account { id: string; name: string; category: string }
export interface Entry { id: string; kind: string; amount: string; note: string; time: string }
export interface Reference { date: string; amount: string }
export interface DayView { date: string; income: string; expense: string; net: string; balance: string | null; reference: Reference | null; entries: Entry[] }
export interface PageQuery {status?: 'pending' | 'confirmed' | 'ignored'; source?: 'wechat' | 'alipay' | 'bank' | 'notification' | 'manual'; account_id?: string; category_id?: string; from_date?: string; until_date?: string; merchant?: string; keyword?: string; offset: number; limit: number}
export interface PageEntry {id: string; status: 'pending' | 'confirmed' | 'ignored'; kind: string; occurred_at: string; amount: string | null; merchant: string | null; description: string | null; edit_version: number; reversal_of: string | null; account_id: string | null; category_id: string | null; original_transaction_id: string | null; sources: string[]}
export interface EntryPage {items: PageEntry[]; total: number; offset: number; limit: number}
export interface CommandError {code: 'invalid_input' | 'not_found' | 'conflict' | 'storage_error' | 'busy'; message: string}
export interface PendingEdit { id: string; expected_version: number; account_id: string; date: string; amount: string; kind: string; merchant: string; description: string; category_id: string | null; original_transaction_id: string | null }
export interface BillRow { id: string; row: number; date: string; amount: string; kind: string; merchant: string; description: string; decision: 'new' | 'exact_duplicate' | 'suspected_duplicate'; duplicate_id: string | null; warnings: string[] }
export interface BillPreview { preview_id: string; rows: BillRow[]; errors: {row: number; message: string}[]; new_count: number; exact_duplicates: number; suspected_duplicates: number; pending_count: number }
export interface CategoryOption { id: string; name: string }
export interface TransactionDto { id: string; kind: string; status: 'pending'|'confirmed'|'ignored'; amount: string; date: string; account_id: string; to_account_id?: string|null; category_id?: string|null; merchant?: string|null; description?: string|null; original_transaction_id?: string|null; edit_version: number }
export interface TransactionDetail { transaction: TransactionDto; edit_version: number; reversal_of: string|null; reversal_id: string|null; related_ids: string[]; sources: {source:string; external_id?:string|null; digest?:string|null}[]; history: {id:string; action:string; time:string; detail:string}[] }
interface RawTransactionDetail extends Omit<TransactionDto, 'to_account_id'> { destination_id: string|null; reversal_of: string|null; reversal_id: string|null; linked_ids: string[]; sources: {source:string;external_id?:string|null;digest?:string|null}[]; history: [string,number][] }
export const api = {
  previewBill: (provider: string, bytes: number[]) => invoke<BillPreview>('preview_bill', { provider, bytes }),
  stageBill: (previewId: string, accountId: string, ids: string[], edits: PendingEdit[]) => invoke<string[]>('stage_bill', { previewId, accountId, ids, edits }),
  confirmPending: (ids: string[], edits: PendingEdit[] = []) => invoke<void>('confirm_pending', { ids, edits }),
  editPending: (edit: PendingEdit) => invoke<void>('edit_pending', { edit }),
  categories: () => invoke<CategoryOption[]>('list_categories'),
  queryEntries: (query: PageQuery) => invoke<EntryPage>('query_entries', { query }),
  pending: (query: Omit<PageQuery, 'status'> & { status?: 'pending' | 'ignored' }) => invoke<EntryPage>('query_entries', { query: { ...query, status: query.status ?? 'pending' } }),
  ignorePending: (ids: string[]) => invoke<void>('ignore_pending', { ids }),
  restorePending: (id: string) => invoke<void>('restore_pending', { id }),
  accounts: () => invoke<Account[]>('list_accounts'),
  createAccount: (name: string, category: string) => invoke<Account>('create_account', { name, category }),
  updatePlatform: (accountId: string, name: string, category: string) => invoke<void>('update_platform', { accountId, name, category }),
  day: (accountId: string, date: string) => invoke<DayView>('get_day', { accountId, date }),
  saveEntry: (accountId: string, date: string, amount: string, note: string, income: boolean) => invoke<void>('save_entry', { accountId, date, amount, note, income }),
  setReference: (accountId: string, date: string, amount: string) => invoke<void>('set_reference', { accountId, date, amount }),
  clearReference: (accountId: string) => invoke<void>('clear_reference', { accountId }),
  listCategories: () => invoke<CategoryOption[]>('list_categories'),
  transactionDetail: async (id: string): Promise<TransactionDetail> => {
    const raw = await invoke<RawTransactionDetail>('transaction_detail', { id });
    return {
      transaction: { ...raw, to_account_id: raw.destination_id }, edit_version: raw.edit_version,
      reversal_of: raw.reversal_of, reversal_id: raw.reversal_id, related_ids: raw.linked_ids,
      sources: raw.sources,
      history: raw.history.map(([action, micros], index) => ({ id: String(index), action, time: new Date(micros / 1000).toISOString(), detail: action })),
    };
  },
  saveTransaction: async (payload: Record<string, unknown>): Promise<{id:string}> => ({ id: await invoke<string>('write_transaction', {request: {edit: {id: payload.id ?? '', expected_version: payload.expected_version ?? 0, account_id: payload.account_id, date: payload.date, amount: payload.amount, kind: payload.kind, merchant: payload.merchant ?? '', description: payload.description ?? '', category_id: payload.category_id ?? null, original_transaction_id: payload.original_transaction_id ?? null}, destination_id: payload.to_account_id ?? null}}) }),
  reverseTransaction: async (id: string, reason: string): Promise<{id:string}> => ({ id: await invoke<string>('reverse_transaction', {id, reason}) }),
};
