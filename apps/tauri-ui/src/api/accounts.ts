import { invoke } from './client';
export interface Account {
  id: string;
  name: string;
  category: string;
}
export const accountsApi = {
  list: () => invoke<Account[]>('list_accounts'),
  create: (name: string, category: string) => invoke<Account>('create_account', { name, category }),
  update: (accountId: string, name: string, category: string) =>
    invoke<void>('update_platform', { accountId, name, category }),
};
