import { invoke } from '../../api/client';
import type { DataFile } from '../../api/data';
export const isAndroid = () => /android/i.test(navigator.userAgent);
export async function saveFile(file: DataFile) {
  if (isAndroid()) {
    await invoke<void>('save_data_file', { name: file.name, mime: file.mime, data: file.data });
    return;
  }
  const bytes = Uint8Array.from(atob(file.data), (c) => c.charCodeAt(0));
  const url = URL.createObjectURL(new Blob([bytes], { type: file.mime }));
  const link = document.createElement('a');
  link.href = url;
  link.download = file.name;
  document.body.append(link);
  link.click();
  link.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
}
export async function readFile(file: File): Promise<string> {
  if (file.size > 32 * 1024 * 1024) throw Error('文件超过 32 MB');
  const bytes = new Uint8Array(await file.arrayBuffer());
  let text = '';
  for (let i = 0; i < bytes.length; i += 8192)
    text += String.fromCharCode(...bytes.subarray(i, i + 8192));
  return btoa(text);
}
export const pickBackup = () => invoke<{ data: string }>('open_backup_file');
