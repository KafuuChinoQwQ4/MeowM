import { invoke } from '@tauri-apps/api/core';
export interface Account { id: string; name: string; category: string }
export interface Entry { id: string; kind: string; amount: string; note: string; time: string }
export interface Reference { date: string; amount: string }
export interface DayView { date: string; income: string; expense: string; net: string; balance: string | null; reference: Reference | null; entries: Entry[] }
export interface PageQuery {status?: 'pending' | 'confirmed' | 'ignored'; account_id?: string; category_id?: string; from_date?: string; until_date?: string; merchant?: string; keyword?: string; offset: number; limit: number}
export interface PageEntry {id: string; status: 'pending' | 'confirmed' | 'ignored'; kind: string; occurred_at: string; amount: string | null; merchant: string | null; description: string | null; edit_version: number; reversal_of: string | null}
export interface EntryPage {items: PageEntry[]; total: number; offset: number; limit: number}
export interface CommandError {code: 'invalid_input' | 'not_found' | 'conflict' | 'storage_error' | 'busy'; message: string}
export const api = {
  queryEntries: (query: PageQuery) => invoke<EntryPage>('query_entries', { query }),
  accounts: () => invoke<Account[]>('list_accounts'),
  createAccount: (name: string, category: string) => invoke<Account>('create_account', { name, category }),
  updatePlatform: (accountId: string, name: string, category: string) => invoke<void>('update_platform', { accountId, name, category }),
  day: (accountId: string, date: string) => invoke<DayView>('get_day', { accountId, date }),
  saveEntry: (accountId: string, date: string, amount: string, note: string, income: boolean) => invoke<void>('save_entry', { accountId, date, amount, note, income }),
  setReference: (accountId: string, date: string, amount: string) => invoke<void>('set_reference', { accountId, date, amount }),
  clearReference: (accountId: string) => invoke<void>('clear_reference', { accountId }),
};
