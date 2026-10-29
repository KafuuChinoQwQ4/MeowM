import type { Account } from '../../api/accounts';

export type ImportProvider = 'wechat' | 'alipay' | 'bank';

export interface ImportSource {
  id: string;
  label: string;
  provider: ImportProvider | null;
  accountId: string | null;
  category: string;
  supported: boolean;
}

export function providerForAccount(account: Account): ImportProvider | null {
  if (account.category === 'bank') return 'bank';
  const name = account.name.toLowerCase();
  if (name.includes('支付宝') || name.includes('alipay')) return 'alipay';
  if (name.includes('微信') || name.includes('wechat')) return 'wechat';
  if (/银行|農商|农商|信用社|信用合作|信用联社|bank/i.test(name)) {
    return 'bank';
  }
  return null;
}

export function importSources(accounts: Account[]): ImportSource[] {
  const sources: ImportSource[] = accounts.map((account) => {
    const provider = providerForAccount(account);
    return {
      id: account.id,
      label: account.name,
      provider,
      accountId: account.id,
      category: account.category ?? 'other',
      supported: provider !== null,
    };
  });
  const labels: Record<ImportProvider, string> = {
    wechat: '微信账单格式（未关联平台）',
    alipay: '支付宝账单格式（未关联平台）',
    bank: '银行卡账单格式（未关联平台）',
  };
  for (const provider of ['wechat', 'alipay', 'bank'] as const) {
    if (!sources.some((source) => source.provider === provider)) {
      sources.push({
        id: provider,
        label: labels[provider],
        provider,
        accountId: null,
        category: provider === 'bank' ? 'bank' : 'wallet',
        supported: true,
      });
    }
  }
  return sources;
}

export function initialImportSource(
  sources: ImportSource[],
  accounts: Account[],
  accountId: string,
): ImportSource {
  const account = accounts.find((item) => item.id === accountId);
  const accountProvider = account ? providerForAccount(account) : null;
  const preferredProvider = accountProvider ?? 'wechat';
  return (
    (accountProvider &&
      sources.find((source) => source.accountId === accountId && source.supported)) ??
    sources.find((source) => source.provider === preferredProvider && source.accountId !== null) ??
    sources.find((source) => source.provider === preferredProvider) ??
    sources.find((source) => source.supported) ??
    sources[0]
  );
}
