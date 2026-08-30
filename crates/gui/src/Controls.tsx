import type { ReactNode } from "react";

/**
 * Replacements for the stock browser controls.
 *
 * `<select>` and `<input type=checkbox>` render with the platform's *web* look,
 * which is the single loudest "this is a web page" tell in the app
 * (design/rubric.md § Controls). Both keep real semantics underneath: the
 * segmented control is a radiogroup of buttons, and the checkbox is a real
 * `<input type=checkbox>` visually replaced rather than reimplemented — so
 * keyboard, screen readers and form semantics are unchanged.
 */

export type Segment<T extends string> = { value: T; label: string };

export function Segmented<T extends string>({
  label,
  value,
  options,
  disabled = false,
  onChange,
}: {
  label: string;
  value: T;
  options: Segment<T>[];
  disabled?: boolean;
  onChange: (v: T) => void;
}) {
  return (
    <div
      role="radiogroup"
      aria-label={label}
      className={`inline-flex gap-0.5 rounded-[7px] border border-separator bg-white/[.06] p-0.5 ${
        disabled ? "opacity-50" : ""
      }`}
    >
      {options.map((o) => {
        const on = o.value === value;
        return (
          <button
            key={o.value}
            type="button"
            role="radio"
            aria-checked={on}
            disabled={disabled}
            onClick={() => onChange(o.value)}
            // `whitespace-nowrap` is not cosmetic: without it a two-word label
            // ("100 MB", "6 months") wraps inside a 22px-tall button at narrow
            // widths, inflating the control to ~40px and breaking the 52px
            // toolbar it sits in.
            className={`h-[22px] whitespace-nowrap rounded-[5px] px-2.5 text-caption font-medium transition-colors duration-fast ease-mac ${
              on
                ? "bg-surface3 text-text shadow-e2"
                : "text-muted hover:text-text"
            }`}
          >
            {o.label}
          </button>
        );
      })}
    </div>
  );
}

/**
 * A 14px rounded square with a drawn checkmark. The real input stays in the
 * DOM (visually hidden, not `display:none`) so it keeps focus, keyboard and
 * assistive-technology behaviour; `peer-*` classes drive the visual state.
 */
export function Checkbox({
  checked,
  onChange,
  label,
}: {
  checked: boolean;
  onChange: () => void;
  label: string;
}) {
  return (
    <span className="relative grid flex-none place-items-center">
      <input
        type="checkbox"
        checked={checked}
        onChange={onChange}
        aria-label={label}
        className="peer h-[14px] w-[14px] cursor-pointer appearance-none rounded-[4px] border border-borderStrong bg-white/[.04] transition-colors duration-fast ease-mac checked:border-accent checked:bg-accent"
      />
      <svg
        viewBox="0 0 14 14"
        aria-hidden="true"
        className="pointer-events-none absolute h-[14px] w-[14px] opacity-0 transition-opacity duration-fast ease-mac peer-checked:opacity-100"
      >
        <path
          d="M3.6 7.2 6 9.5l4.5-5"
          fill="none"
          stroke="#fff"
          strokeWidth="1.75"
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      </svg>
    </span>
  );
}

/** A small stepper-style numeric field, used for the age filter. */
export function NumberField({
  value,
  onChange,
  label,
  placeholder,
  disabled = false,
  suffix,
}: {
  value: string;
  onChange: (v: string) => void;
  label: string;
  placeholder?: string;
  disabled?: boolean;
  suffix?: ReactNode;
}) {
  return (
    <span className="inline-flex items-center gap-1.5">
      <input
        type="number"
        min="0"
        inputMode="numeric"
        value={value}
        disabled={disabled}
        placeholder={placeholder}
        onChange={(e) => onChange(e.target.value)}
        aria-label={label}
        className="h-[22px] w-[58px] rounded-[5px] border border-separator bg-white/[.06] px-2 text-center font-mono text-caption tabular-nums text-text transition-colors duration-fast ease-mac placeholder:text-subtle focus:border-accentText disabled:opacity-50"
      />
      {suffix && <span className="text-muted text-caption">{suffix}</span>}
    </span>
  );
}
