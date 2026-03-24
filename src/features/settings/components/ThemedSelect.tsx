import Select from "react-select";
import type { CSSProperties } from "react";
import type { SingleValue } from "react-select";

export interface ThemedSelectOption {
  value: string;
  label: string;
  disabled?: boolean;
}

interface ThemedSelectProps {
  theme: string;
  options: ThemedSelectOption[];
  value: string;
  onChange: (value: string) => void | Promise<void>;
  width?: string;
  nativeStyle?: CSSProperties;
  placeholder?: string;
}

const ThemedSelect = ({
  theme,
  options,
  value,
  onChange,
  width = "160px",
  nativeStyle,
  placeholder
}: ThemedSelectProps) => {
  const selected = options.find((option) => option.value === value) ?? null;
  const isMacosTheme = theme === "macos";

  if (!isMacosTheme) {
    return (
      <select
        className="search-input"
        style={nativeStyle}
        value={value}
        onChange={(e) => {
          void onChange(e.target.value);
        }}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value} disabled={option.disabled}>
            {option.label}
          </option>
        ))}
      </select>
    );
  }

  return (
    <div style={{ width }}>
      <Select
        classNamePrefix="tiez-select"
        options={options}
        value={selected}
        placeholder={placeholder}
        isSearchable={false}
        isOptionDisabled={(option) => !!option.disabled}
        menuPortalTarget={document.body}
        menuPosition="fixed"
        onChange={(option: SingleValue<ThemedSelectOption>) => {
          if (!option) return;
          void onChange(option.value);
        }}
        styles={{
          control: (base, state) => ({
            ...base,
            minHeight: "34px",
            borderRadius: "var(--select-control-radius)",
            border: state.isFocused ? "var(--select-control-focus-border)" : "var(--select-control-border)",
            background: "var(--select-control-bg)",
            boxShadow: state.isFocused ? "var(--select-control-focus-shadow)" : "var(--select-control-shadow)",
            cursor: "pointer",
            fontSize: "12px"
          }),
          singleValue: (base) => ({
            ...base,
            color: "var(--select-single-value-color)",
            fontWeight: 600
          }),
          placeholder: (base) => ({
            ...base,
            color: "var(--select-placeholder-color)",
            fontWeight: 500
          }),
          dropdownIndicator: (base) => ({
            ...base,
            color: "var(--select-indicator-color)",
            padding: "0 8px"
          }),
          indicatorSeparator: () => ({
            display: "none"
          }),
          menuPortal: (base) => ({
            ...base,
            zIndex: 99999
          }),
          menu: (base) => ({
            ...base,
            marginTop: "4px",
            borderRadius: "10px",
            overflow: "hidden",
            border: "var(--select-menu-border)",
            background: "var(--select-menu-bg)",
            boxShadow: "var(--select-menu-shadow)"
          }),
          option: (base, state) => ({
            ...base,
            fontSize: "12px",
            cursor: "pointer",
            background: state.isSelected
              ? "var(--select-option-selected-bg)"
              : state.isFocused
                ? "var(--select-option-focus-bg)"
                : "transparent",
            color: state.isSelected
              ? "var(--select-option-selected-color)"
              : state.isFocused
                ? "var(--select-option-focus-color)"
                : "var(--select-option-color)"
          })
        }}
      />
    </div>
  );
};

export default ThemedSelect;
