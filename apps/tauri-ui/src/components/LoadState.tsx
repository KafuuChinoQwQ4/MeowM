export function LoadState({
  loading,
  error,
  reload,
}: {
  loading: boolean;
  error: string;
  reload: () => void;
}) {
  if (loading)
    return (
      <div className="empty-state loading-state">
        <span className="loader" />
        <p>正在加载…</p>
      </div>
    );
  if (error)
    return (
      <div className="feedback" role="alert">
        {error}{' '}
        <button className="text-button" onClick={reload}>
          重新加载
        </button>
      </div>
    );
  return null;
}
