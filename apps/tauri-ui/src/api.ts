import { invoke } from '@tauri-apps/api/core';
export interface Account { id: string; name: string; category: string }
export interface Entry { id: string; kind: string; amount: string; note: string; time: string }
export interface Reference { date: string; amount: string }
export interface DayView { date: string; income: string; expense: string; net: string; balance: string | null; reference: Reference | null; entries: Entry[] }
export const api = {
  accounts: () => invoke<Account[]>('list_accounts'),
  createAccount: (name: string, category: string) => invoke<Account>('create_account', { name, category }),
  updatePlatform: (accountId: string, name: string, category: string) => invoke<void>('update_platform', { accountId, name, category }),
  day: (accountId: string, date: string) => invoke<DayView>('get_day', { accountId, date }),
  saveEntry: (accountId: string, date: string, amount: string, note: string, income: boolean) => invoke<void>('save_entry', { accountId, date, amount, note, income }),
  setReference: (accountId: string, date: string, amount: string) => invoke<void>('set_reference', { accountId, date, amount }),
  clearReference: (accountId: string) => invoke<void>('clear_reference', { accountId }),
};
