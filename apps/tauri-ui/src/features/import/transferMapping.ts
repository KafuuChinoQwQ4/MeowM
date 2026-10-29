import type { Account } from '../../api/accounts';
import type { BillRow } from '../../api/imports';

export interface TransferMapping {
  account_id: string;
  destination_id: string;
}

function normalizedName(value: string): string {
  return value
    .normalize('NFKC')
    .toLocaleLowerCase()
    .replace(/[\p{P}\p{Z}\s]/gu, '');
}

function matchingCounterparty(
  row: BillRow,
  accounts: Account[],
  statementAccountId: string,
): string {
  const merchant = normalizedName(row.merchant);
  if (!merchant) return '';
  const matches = accounts.filter((account) => {
    if (account.id === statementAccountId) return false;
    const name = normalizedName(account.name);
    return name.length >= 2 && (merchant === name || merchant.includes(name));
  });
  return matches.length === 1 ? matches[0].id : '';
}

export function defaultTransferMapping(
  row: BillRow,
  accounts: Account[],
  statementAccountId: string,
): TransferMapping {
  const counterparty = matchingCounterparty(row, accounts, statementAccountId);
  if (row.transfer_direction === 'outgoing') {
    return { account_id: statementAccountId, destination_id: counterparty };
  }
  if (row.transfer_direction === 'incoming') {
    return { account_id: counterparty, destination_id: statementAccountId };
  }
  return { account_id: '', destination_id: '' };
}
