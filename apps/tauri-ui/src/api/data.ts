import { invoke } from './client';
import type { PageQuery } from './entries';
export interface DataFile {
  name: string;
  mime: string;
  data: string;
  count: number | null;
}
export interface RestorePreview {
  id: string;
  backup: {
    format_version: number;
    schema_version: number;
    created_at: string;
    counts: [string, number][];
  };
  current_counts: [string, number][];
}
export const dataApi = {
  backup: (password: string) => invoke<DataFile>('create_backup', { password }),
  inspect: (data: string, password: string) =>
    invoke<RestorePreview>('inspect_backup', { data, password }),
  restore: (previewId: string) => invoke<void>('restore_backup', { previewId, confirmed: true }),
  export: (query: PageQuery, format: 'csv' | 'json', bom: boolean) =>
    invoke<DataFile>('export_data', {
      query,
      format,
      bom,
      includeUnconfirmed: query.status !== 'confirmed',
    }),
};
