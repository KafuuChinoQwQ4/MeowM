import { invoke } from './client';
export interface Entry {
  id: string;
  kind: string;
  amount: string;
  note: string;
  time: string;
}
export interface Reference {
  date: string;
  amount: string;
}
export interface DayView {
  date: string;
  income: string;
  expense: string;
  net: string;
  balance: string | null;
  reference: Reference | null;
  entries: Entry[];
}

export const ledgerApi = {
  day: (accountId: string, date: string) => invoke<DayView>('get_day', { accountId, date }),
  setReference: (accountId: string, date: string, amount: string) =>
    invoke<void>('set_reference', { accountId, date, amount }),
  clearReference: (accountId: string) => invoke<void>('clear_reference', { accountId }),
};
