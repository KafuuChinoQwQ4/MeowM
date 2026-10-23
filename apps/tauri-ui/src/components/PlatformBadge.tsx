import { platformThemeStyle, resolvePlatformTheme } from '../shared/platformThemes';

export function PlatformBadge({ name, category }: { name: string; category: string }) {
  const theme = resolvePlatformTheme(name, category);
  return (
    <strong
      className="platform-badge"
      data-platform={theme.id}
      style={platformThemeStyle(theme)}
      title={name}
    >
      {name || '平台名称'}
    </strong>
  );
}
