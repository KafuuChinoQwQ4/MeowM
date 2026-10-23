import { useEffect, useId, useRef, useState } from 'react';
import { Field } from '../../components/Fields';
import { platformCategories } from '../../shared/labels';
import { platformThemeStyle, searchPlatformThemes } from '../../shared/platformThemes';

export function PlatformNamePicker({
  value,
  onChange,
  onSelect,
}: {
  value: string;
  onChange: (name: string) => void;
  onSelect: (name: string, category: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const [showAll, setShowAll] = useState(false);
  const [active, setActive] = useState(-1);
  const listId = useId();
  const root = useRef<HTMLDivElement>(null);
  const options = searchPlatformThemes(showAll ? '' : value);
  const customName = value.trim();
  const hasCustom = customName && !options.some((theme) => theme.name === customName);
  const count = options.length + (hasCustom ? 1 : 0);

  useEffect(() => {
    if (open && active >= 0) {
      document.getElementById(`${listId}-${active}`)?.scrollIntoView({ block: 'nearest' });
    }
  }, [active, listId, open]);

  function choose(index: number) {
    const theme = options[index];
    if (theme) onSelect(theme.name, theme.category);
    else if (hasCustom) onChange(customName);
    setOpen(false);
    setActive(-1);
  }

  return (
    <div
      className="platform-name-picker"
      ref={root}
      onBlur={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget)) setOpen(false);
      }}
    >
      <div className="platform-name-input">
        <Field
          label="平台名称"
          role="combobox"
          aria-autocomplete="list"
          aria-expanded={open}
          aria-controls={open ? listId : undefined}
          aria-activedescendant={open && active >= 0 ? `${listId}-${active}` : undefined}
          required
          maxLength={40}
          autoFocus
          value={value}
          onFocus={() => {
            setOpen(true);
            setShowAll(false);
            setActive(-1);
          }}
          onChange={(event) => {
            onChange(event.target.value);
            setOpen(true);
            setShowAll(false);
            setActive(-1);
          }}
          onKeyDown={(event) => {
            if (event.nativeEvent.isComposing) return;
            if (event.key === 'ArrowDown' || event.key === 'ArrowUp') {
              event.preventDefault();
              setOpen(true);
              setActive((current) => {
                if (!count) return -1;
                if (current < 0) return event.key === 'ArrowDown' ? 0 : count - 1;
                return (current + (event.key === 'ArrowDown' ? 1 : -1) + count) % count;
              });
            } else if (event.key === 'Enter' && open && active >= 0) {
              event.preventDefault();
              choose(active);
            } else if (event.key === 'Escape' && open) {
              event.preventDefault();
              event.stopPropagation();
              setOpen(false);
            }
          }}
          placeholder="输入名称筛选，或直接填写其他 App"
        />
        <button
          className="platform-list-toggle"
          type="button"
          aria-label={open ? '收起平台列表' : '展开平台列表'}
          aria-expanded={open}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => {
            root.current?.querySelector('input')?.focus();
            setOpen(!open);
            setShowAll(true);
            setActive(-1);
          }}
        >
          <svg viewBox="0 0 24 24" aria-hidden="true">
            <path d="m6 9 6 6 6-6" />
          </svg>
        </button>
      </div>
      {open && (
        <div className="platform-suggestions" role="listbox" aria-label="平台候选" id={listId}>
          {options.map((theme, index) => (
            <button
              key={theme.id}
              id={`${listId}-${index}`}
              type="button"
              role="option"
              aria-selected={active === index}
              tabIndex={-1}
              className="platform-suggestion"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(index)}
            >
              <strong className="platform-badge" style={platformThemeStyle(theme)}>
                {theme.name}
              </strong>
              <span>{platformCategories[theme.category] ?? '其他'}</span>
            </button>
          ))}
          {hasCustom && (
            <button
              id={`${listId}-${options.length}`}
              type="button"
              role="option"
              aria-selected={active === options.length}
              tabIndex={-1}
              className="platform-suggestion custom-platform"
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => choose(options.length)}
            >
              使用“{customName}”
            </button>
          )}
        </div>
      )}
    </div>
  );
}
