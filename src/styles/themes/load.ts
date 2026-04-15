import "./index.css";
import "./dark.css";

// Load every theme stylesheet in this directory automatically so a new theme
// CSS file does not require updating the entry imports.
void import.meta.glob(
  ["./*.css", "!./index.css", "!./dark.css"],
  { eager: true }
);
