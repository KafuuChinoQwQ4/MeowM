import { invoke } from './client';
export interface Account {
  id: string;
  name: string;
  category: string;
  currency_code: string;
  currency_exponent: number;
}
export const CURRENCIES = [
  ['CNY', '人民币'],
  ['USD', '美元'],
  ['EUR', '欧元'],
  ['JPY', '日元'],
  ['HKD', '港币'],
  ['GBP', '英镑'],
  ['KRW', '韩元'],
  ['AUD', '澳元'],
  ['CAD', '加元'],
  ['SGD', '新加坡元'],
  ['CHF', '瑞士法郎'],
] as const;
export const accountsApi = {
  list: () => invoke<Account[]>('list_accounts'),
  create: (name: string, category: string, currencyCode = 'CNY') =>
    invoke<Account>('create_account', { name, category, currencyCode }),
  update: (accountId: string, name: string, category: string) =>
    invoke<void>('update_platform', { accountId, name, category }),
  delete: (accountId: string) => invoke<void>('delete_platform', { accountId }),
};
