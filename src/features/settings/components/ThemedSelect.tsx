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
  colorMode: string;
  options: ThemedSelectOption[];
  value: string;
  onChange: (value: string) => void | Promise<void>;
  width?: string;
  nativeStyle?: CSSProperties;
}

const ThemedSelect = ({
  theme,
  colorMode,
  options,
  value,
  onChange,
  width = "160px",
  nativeStyle
}: ThemedSelectProps) => {
  const selected = options.find((option) => option.value === value) ?? options[0];
  const isMacosTheme = theme === "macos";
  const isDarkMode =
    colorMode === "dark" ||
    (colorMode === "system" && document.documentElement.classList.contains("dark-mode"));

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
            borderRadius: "10px",
            border: state.isFocused
              ? "1px solid rgba(10, 132, 255, 0.6)"
              : isDarkMode
                ? "1px solid rgba(255,255,255,0.16)"
                : "1px solid rgba(60,60,67,0.24)",
            background: isDarkMode ? "rgba(58,60,68,0.7)" : "rgba(255,255,255,0.9)",
            boxShadow: state.isFocused
              ? "0 0 0 3px rgba(10,132,255,0.2)"
              : isDarkMode
                ? "inset 0 1px 0 rgba(255,255,255,0.08)"
                : "inset 0 1px 0 rgba(255,255,255,0.72)",
            cursor: "pointer",
            fontSize: "12px"
          }),
          singleValue: (base) => ({
            ...base,
            color: isDarkMode ? "#f5f5f7" : "var(--text-primary)",
            fontWeight: 600
          }),
          dropdownIndicator: (base) => ({
            ...base,
            color: isDarkMode ? "#f5f5f7" : "#323a45",
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
            border: isDarkMode
              ? "1px solid rgba(255,255,255,0.14)"
              : "1px solid rgba(60,60,67,0.2)",
            background: isDarkMode ? "rgba(62,64,74,0.98)" : "rgba(255,255,255,0.98)",
            boxShadow: isDarkMode
              ? "0 8px 20px rgba(0,0,0,0.35)"
              : "0 8px 20px rgba(15,18,26,0.16)"
          }),
          option: (base, state) => ({
            ...base,
            fontSize: "12px",
            cursor: "pointer",
            background: state.isSelected
              ? "rgba(10,132,255,0.34)"
              : state.isFocused
                ? isDarkMode
                  ? "rgba(255,255,255,0.08)"
                  : "rgba(10,132,255,0.12)"
                : "transparent",
            color: isDarkMode ? "#f5f5f7" : "var(--text-primary)"
          })
        }}
      />
    </div>
  );
};

export default ThemedSelect;
