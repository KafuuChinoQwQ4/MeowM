import type { InputHTMLAttributes, SelectHTMLAttributes } from 'react';
export function Field({
  label,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string }) {
  return (
    <label className="form-field">
      {label}
      <input autoComplete="off" {...props} />
    </label>
  );
}
export function SelectField({
  label,
  options,
  ...props
}: SelectHTMLAttributes<HTMLSelectElement> & { label: string; options: [string, string][] }) {
  return (
    <label className="form-field">
      {label}
      <select aria-label={label} {...props}>
        {options.map(([id, name]) => (
          <option key={id} value={id}>
            {name}
          </option>
        ))}
      </select>
    </label>
  );
}
export function FormError({ message }: { message?: string }) {
  return message ? (
    <p className="form-error" role="alert">
      {message}
    </p>
  ) : null;
}
