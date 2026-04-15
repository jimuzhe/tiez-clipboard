export interface Announcement {
  id: string;
  type: "info" | "warning" | "update" | "promotion";
  title: string;
  message: string;
  textColor?: string;
  link?: string;
  linkText?: string;
  versionTarget?: string; // If set, only show for this version
  minVersion?: string; // If set, show for versions >= this
  maxVersion?: string; // If set, show for versions <= this
  forceOpen?: boolean; // If true, behaves like a modal/pop-up
  createdAt?: string;
}
