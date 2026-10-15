export function Pager({
  page,
  pages,
  onChange,
  disabled = false,
}: {
  page: number;
  pages: number;
  onChange: (page: number) => void;
  disabled?: boolean;
}) {
  return (
    <div className="record-pager">
      <button
        className="text-button"
        disabled={disabled || page === 0}
        onClick={() => onChange(page - 1)}
      >
        上一页
      </button>
      <span>
        {page + 1} / {Math.max(1, pages)}
      </span>
      <button
        className="text-button"
        disabled={disabled || page >= pages - 1}
        onClick={() => onChange(page + 1)}
      >
        下一页
      </button>
    </div>
  );
}
