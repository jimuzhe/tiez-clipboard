import "./index.css";
import "./dark.css";

void import.meta.glob(
  ["./*.css", "!./index.css", "!./dark.css"],
  { eager: true }
);
