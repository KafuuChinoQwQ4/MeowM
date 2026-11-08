export function ReadState({
  loading,
  error,
  reload,
}: {
  loading: boolean;
  error: string;
  reload: () => void;
}) {
  return (
    <>
      {loading && <p role="status">正在读取数据…</p>}
      {error && (
        <p role="alert">
          {error}{' '}
          <button className="button" type="button" onClick={reload}>
            重试
          </button>
        </p>
      )}
    </>
  );
}
