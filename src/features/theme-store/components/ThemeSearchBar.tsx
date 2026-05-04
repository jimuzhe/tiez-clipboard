import { memo, useState, useCallback } from "react";
import { Search } from "lucide-react";

interface ThemeSearchBarProps {
  onSearch: (q: string) => void;
  placeholder?: string;
}

const ThemeSearchBar = ({ onSearch, placeholder }: ThemeSearchBarProps) => {
  const [value, setValue] = useState("");

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        onSearch(value.trim());
      }
    },
    [value, onSearch]
  );

  return (
    <div className="theme-store-search">
      <Search size={12} style={{ color: "var(--text-muted)", flexShrink: 0 }} />
      <input
        type="text"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        placeholder={placeholder || "Search themes..."}
      />
    </div>
  );
};

export default memo(ThemeSearchBar);
