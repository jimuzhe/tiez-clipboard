import { useEffect, useMemo, useState } from "react";
import type { DragEvent } from "react";
import { invoke, convertFileSrc } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { open } from "@tauri-apps/plugin-dialog";
import { Plus, X } from "lucide-react";
import { motion, AnimatePresence } from "framer-motion";

interface EmojiPanelProps {
  t: (key: string) => string;
  favorites: string[];
  setFavorites: (val: string[] | ((prev: string[]) => string[])) => void;
  activeTab: "emoji" | "favorites";
  setActiveTab: (val: "emoji" | "favorites") => void;
  saveSetting: (key: string, val: string) => void;
}

type EmojiGroup = { name: string; emojis: string[] };
type EmojiData = { groups?: EmojiGroup[] };

const FALLBACK_GROUPS: EmojiGroup[] = [
  {
    name: "常用",
    emojis: ["😀", "😁", "😂", "🤣", "😊", "😍", "😘", "😎", "🤔", "😅", "😭", "😡", "👍", "👎", "🙏", "👏", "🎉", "🔥", "💯", "✨", "👌", "😴", "🥳", "🤩", "😬", "😇", "🤝", "🙌"]
  },
  {
    name: "表情",
    emojis: ["🙂", "😇", "🙃", "😉", "😌", "🤗", "🤩", "🥳", "😴", "😪", "😤", "😱", "🤯", "😵", "🤐", "🫠", "🫡", "🫣", "😐", "😑", "😶", "🙄", "😮", "😯", "😲", "🥺", "😢", "😥", "😓", "😕"]
  },
  {
    name: "手势",
    emojis: ["👌", "✌️", "🤞", "🤟", "🤘", "🤙", "👊", "✊", "🤚", "🖐️", "✋", "👋", "🫶", "👉", "👈", "👇", "👆", "🫵", "🤝", "🙌", "🤲", "🤜", "🤛", "🫰", "🤌"]
  },
  {
    name: "人物",
    emojis: ["👨‍💻", "👩‍💻", "👨‍🎨", "👩‍🎨", "👨‍🚀", "👩‍🚀", "👨‍🍳", "👩‍🍳", "👨‍⚕️", "👩‍⚕️", "👨‍🏫", "👩‍🏫", "🧑‍💼", "🧑‍🔧", "🧑‍🎧", "🧑‍🚒"]
  },
  {
    name: "动物",
    emojis: ["🐶", "🐱", "🐭", "🐹", "🐰", "🦊", "🐻", "🐼", "🐯", "🦁", "🐮", "🐷", "🐸", "🐵", "🐔", "🐧", "🐦", "🐤", "🐺", "🦄"]
  },
  {
    name: "美食",
    emojis: ["🍎", "🍐", "🍊", "🍋", "🍉", "🍇", "🍓", "🍒", "🍍", "🥭", "🍔", "🍟", "🍕", "🌭", "🍣", "🍤", "🍜", "🍲", "🍰", "🍩"]
  },
  {
    name: "活动",
    emojis: ["⚽", "🏀", "🏈", "⚾", "🎾", "🏐", "🏓", "🏸", "🥊", "🏆", "🎯", "🎮", "🎲", "🎹", "🎸", "🎤", "🎧", "🏃", "🚴", "🧘"]
  },
  {
    name: "旅行",
    emojis: ["🚗", "🚕", "🚌", "🚎", "🏎️", "🚓", "🚑", "🚒", "🚀", "✈️", "🛫", "🛬", "🚢", "⛵", "🗺️", "🧭", "🏝️", "⛰️", "🌋", "🏜️"]
  },
  {
    name: "物品",
    emojis: ["📱", "💻", "🖥️", "⌨️", "🖱️", "📷", "🎥", "📺", "🔦", "💡", "🔋", "🔌", "📦", "📌", "✏️", "📚", "🧰", "🧲", "🧯", "🧪"]
  },
  {
    name: "符号",
    emojis: ["❤️", "🧡", "💛", "💚", "💙", "💜", "🖤", "🤍", "🤎", "💔", "❗", "❓", "✅", "❌", "⚠️", "⭕", "💯", "✨", "⭐", "🌟"]
  }
];

const IMAGE_EXTS = new Set(["png", "jpg", "jpeg", "webp", "gif", "avif"]);

const normalizePath = (path: string) => path.trim();

const isImagePath = (path: string) => {
  const ext = path.split(".").pop()?.toLowerCase() || "";
  return IMAGE_EXTS.has(ext);
};

const isImageFile = (file: File) => {
  if (file.type && file.type.startsWith("image/")) return true;
  const ext = file.name.split(".").pop()?.toLowerCase() || "";
  return IMAGE_EXTS.has(ext);
};

const fileToDataUrl = (file: File) =>
  new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => {
      if (typeof reader.result === "string") resolve(reader.result);
      else reject(new Error("Invalid file result"));
    };
    reader.onerror = () => reject(reader.error || new Error("File read failed"));
    reader.readAsDataURL(file);
  });

const parseSrcset = (srcset: string) => {
  const first = srcset.split(",")[0]?.trim() || "";
  if (!first) return "";
  return first.split(/\s+/)[0] || "";
};

const decodeUriComponentSafely = (value: string, rounds = 2) => {
  let out = value;
  for (let i = 0; i < rounds; i++) {
    if (!/%[0-9a-fA-F]{2}/.test(out)) break;
    try {
      const decoded = decodeURIComponent(out);
      if (decoded === out) break;
      out = decoded;
    } catch {
      break;
    }
  }
  return out;
};

const stripWrappingQuotes = (value: string) => value.trim().replace(/^['"]+|['"]+$/g, "");

const looksLikeAbsolutePath = (value: string) =>
  /^[a-zA-Z]:[\\/]/.test(value) ||
  value.startsWith("/") ||
  value.startsWith("\\\\") ||
  value.startsWith("//");

const normalizeDroppedLocalPath = (rawValue: string) => {
  const trimmed = stripWrappingQuotes(decodeUriComponentSafely(rawValue.trim()));
  if (!trimmed) return null;

  if (/^file:/i.test(trimmed)) {
    const directPath = decodeUriComponentSafely(trimmed.replace(/^file:\/+/i, ""));
    if (/^[a-zA-Z]:[\\/]/.test(directPath)) {
      return directPath;
    }

    try {
      const parsed = new URL(trimmed);
      const decodedPath = decodeUriComponentSafely(parsed.pathname || "");
      if (!decodedPath && !parsed.host) return null;

      if (parsed.host && parsed.host !== "localhost") {
        if (/^[a-zA-Z]:$/.test(parsed.host)) {
          return `${parsed.host}${decodedPath}`;
        }
        return `//${parsed.host}${decodedPath}`;
      }

      if (/^\/[a-zA-Z]:/.test(decodedPath)) {
        return decodedPath.slice(1);
      }

      return decodedPath || null;
    } catch {
      let fallback = trimmed.replace(/^file:\/+/i, "");
      fallback = decodeUriComponentSafely(fallback);
      if (/^[a-zA-Z]:[\\/]/.test(fallback)) return fallback;
      if (fallback.startsWith("/") && /^\/[a-zA-Z]:/.test(fallback)) return fallback.slice(1);
      return fallback ? `//${fallback}` : null;
    }
  }

  return looksLikeAbsolutePath(trimmed) ? trimmed : null;
};

const parseDownloadUrl = (value: string) => {
  const trimmed = value.trim();
  if (!trimmed) return "";
  const firstColon = trimmed.indexOf(":");
  if (firstColon < 0) return trimmed;
  const secondColon = trimmed.indexOf(":", firstColon + 1);
  if (secondColon < 0) return trimmed;
  return trimmed.slice(secondColon + 1).trim();
};

const normalizeRemoteDropUrl = (rawValue: string) => {
  const trimmed = stripWrappingQuotes(decodeUriComponentSafely(rawValue.trim()));
  if (!trimmed || /^blob:/i.test(trimmed)) return null;
  if (/^https?:\/\//i.test(trimmed)) return trimmed;
  if (trimmed.startsWith("//")) return `https:${trimmed}`;
  return null;
};

const collectImageUrlsFromHtml = (html: string) => {
  try {
    const doc = new DOMParser().parseFromString(html, "text/html");
    const urls: string[] = [];
    const baseCandidates = [
      doc.querySelector("base[href]")?.getAttribute("href") || "",
      ...Array.from(doc.querySelectorAll("a[href]"))
        .map((anchor) => anchor.getAttribute("href") || "")
        .filter(Boolean)
    ];
    const baseUrl = baseCandidates
      .map((value) => normalizeRemoteDropUrl(value))
      .find((value): value is string => !!value);
    const resolveCandidate = (value: string) => {
      const normalizedRemote = normalizeRemoteDropUrl(value);
      if (normalizedRemote) return normalizedRemote;
      if (!baseUrl) return value;
      try {
        return new URL(value, baseUrl).toString();
      } catch {
        return value;
      }
    };
    doc.querySelectorAll("img").forEach((img) => {
      const src = img.getAttribute("src") || "";
      const srcset = img.getAttribute("srcset") || "";
      if (src) urls.push(resolveCandidate(src));
      const srcsetUrl = parseSrcset(srcset);
      if (srcsetUrl) urls.push(resolveCandidate(srcsetUrl));
      ["data-src", "data-original", "data-lazy-src"].forEach((attr) => {
        const value = img.getAttribute(attr) || "";
        if (value) urls.push(resolveCandidate(value));
      });
    });
    doc.querySelectorAll("source").forEach((source) => {
      const src = source.getAttribute("src") || "";
      const srcset = source.getAttribute("srcset") || "";
      if (src) urls.push(resolveCandidate(src));
      const srcsetUrl = parseSrcset(srcset);
      if (srcsetUrl) urls.push(resolveCandidate(srcsetUrl));
    });
    doc.querySelectorAll("a[href]").forEach((anchor) => {
      const href = anchor.getAttribute("href") || "";
      if (href && /\.(png|jpe?g|gif|webp|bmp|ico|svg)([?#]|$)/i.test(href)) {
        urls.push(resolveCandidate(href));
      }
    });
    return urls;
  } catch {
    return [];
  }
};

const getDataTransferStringItems = (dt: DataTransfer | null) =>
  new Promise<Array<{ type: string; value: string }>>((resolve) => {
    if (!dt?.items || dt.items.length === 0) {
      resolve([]);
      return;
    }

    const stringItems = Array.from(dt.items).filter((item) => item.kind === "string");
    if (stringItems.length === 0) {
      resolve([]);
      return;
    }

    let pending = stringItems.length;
    const results: Array<{ type: string; value: string }> = [];

    stringItems.forEach((item) => {
      item.getAsString((value) => {
        if (value) {
          results.push({ type: item.type || "text/plain", value });
        }
        pending -= 1;
        if (pending === 0) {
          resolve(results);
        }
      });
    });
  });

const getDropCandidates = async (dt: DataTransfer | null) => {
  if (!dt) return [];

  const payloads: Array<{ type: string; value: string }> = [];
  const pushPayload = (type: string, value: string) => {
    if (!value) return;
    payloads.push({ type, value });
  };

  pushPayload("text/uri-list", dt.getData("text/uri-list"));
  pushPayload("text/html", dt.getData("text/html"));
  pushPayload("text/plain", dt.getData("text/plain"));
  pushPayload("DownloadURL", dt.getData("DownloadURL"));
  pushPayload("text/x-moz-url", dt.getData("text/x-moz-url"));

  const itemPayloads = await getDataTransferStringItems(dt);
  payloads.push(...itemPayloads);

  const values: string[] = [];
  payloads.forEach(({ type, value }) => {
    if (type === "text/uri-list") {
      value
        .split(/\r?\n/)
        .map((line) => line.trim())
        .filter((line) => line && !line.startsWith("#"))
        .forEach((line) => values.push(line));
      return;
    }

    if (type === "text/html") {
      values.push(...collectImageUrlsFromHtml(value));
      return;
    }

    if (type === "DownloadURL") {
      const parsed = parseDownloadUrl(value);
      if (parsed) values.push(parsed);
      return;
    }

    if (type === "text/x-moz-url") {
      const firstLine = value.split(/\r?\n/).map((line) => line.trim()).find(Boolean);
      if (firstLine) values.push(firstLine);
      return;
    }

    values.push(value.trim());
  });

  return Array.from(new Set(values.map((u) => u.trim()).filter((u) => u.length > 0)));
};

const resolveDropPaths = (payload: unknown): string[] => {
  if (Array.isArray(payload)) {
    return payload.filter((p): p is string => typeof p === "string");
  }
  if (payload && typeof payload === "object" && "paths" in payload) {
    const maybePaths = (payload as { paths?: unknown }).paths;
    if (Array.isArray(maybePaths)) {
      return maybePaths.filter((p): p is string => typeof p === "string");
    }
  }
  return [];
};

const EmojiPanel = ({ t, favorites, setFavorites, activeTab, setActiveTab, saveSetting }: EmojiPanelProps) => {
  const [isDragging, setIsDragging] = useState(false);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [emojiGroups, setEmojiGroups] = useState<EmojiGroup[]>(FALLBACK_GROUPS);

  const flatEmoji = useMemo(() => emojiGroups.flatMap((g) => g.emojis), [emojiGroups]);
  const hasFavorites = favorites.length > 0;

  const persistFavorites = (updater: string[] | ((prev: string[]) => string[])) => {
    setFavorites((prev) => {
      const next = typeof updater === "function" ? updater(prev) : updater;
      saveSetting("app.emoji_favorites", JSON.stringify(next));
      return next;
    });
  };

  const removeFavoritePath = (path: string) => {
    persistFavorites((prev) => prev.filter((p) => p !== path));
    invoke("remove_emoji_favorite", { path }).catch(console.error);
  };

  useEffect(() => {
    if (activeTab !== "favorites") return;

    let cancelled = false;
    invoke<string[]>("list_emoji_favorites")
      .then((diskPaths) => {
        if (cancelled) return;
        const merged = Array.from(new Set([...favorites, ...(Array.isArray(diskPaths) ? diskPaths : [])]));
        if (
          merged.length === favorites.length &&
          merged.every((path, index) => path === favorites[index])
        ) {
          return;
        }
        persistFavorites(merged);
      })
      .catch(console.error);

    return () => {
      cancelled = true;
    };
  }, [activeTab, favorites]);

  const addFavoritePaths = async (paths: string[]) => {
    const normalized = Array.from(
      new Set(
        paths
          .map((path) => normalizeDroppedLocalPath(path) || normalizePath(path))
          .filter((p) => p && isImagePath(p))
      )
    );
    if (normalized.length === 0) return 0;
    const saved = await Promise.all(
      normalized.map(async (path) => {
        try {
          return await invoke<string>("save_emoji_favorite", { sourcePath: path });
        } catch (e) {
          console.warn("Failed to save emoji favorite:", e);
          return null;
        }
      })
    );
    const valid = saved.filter((p): p is string => typeof p === "string" && p.length > 0);
    if (valid.length === 0) return 0;
    persistFavorites((prev) => Array.from(new Set([...prev, ...valid])));
    return valid.length;
  };

  const addFavoriteFiles = async (files: FileList | File[]) => {
    const fileList = files instanceof FileList ? Array.from(files) : files;
    const paths: string[] = [];
    const dataUrlFiles: { dataUrl: string; fileName: string }[] = [];

    for (const file of fileList) {
      if (!isImageFile(file)) continue;
      const filePath = (file as { path?: string }).path;
      if (filePath) {
        paths.push(filePath);
      } else {
        try {
          const dataUrl = await fileToDataUrl(file);
          dataUrlFiles.push({ dataUrl, fileName: file.name });
        } catch (e) {
          console.warn("Failed to read dropped file:", e);
        }
      }
    }

    let addedCount = 0;
    if (paths.length > 0) {
      addedCount += await addFavoritePaths(paths);
    }

    if (dataUrlFiles.length > 0) {
      const saved = await Promise.all(
        dataUrlFiles.map(async ({ dataUrl, fileName }) => {
          try {
            return await invoke<string>("save_emoji_favorite_data_url", { dataUrl, fileName });
          } catch (e) {
            console.warn("Failed to save dropped data url:", e);
            return null;
          }
        })
      );
      const valid = saved.filter((p): p is string => typeof p === "string" && p.length > 0);
      if (valid.length > 0) {
        persistFavorites((prev) => Array.from(new Set([...prev, ...valid])));
        addedCount += valid.length;
      }
    }

    return addedCount;
  };

  const addFavoriteDataUrls = async (dataUrls: string[]) => {
    const normalized = dataUrls.map((url) => url.trim()).filter((url) => url.startsWith("data:"));
    if (normalized.length === 0) return 0;
    const saved = await Promise.all(
      normalized.map(async (dataUrl) => {
        try {
          return await invoke<string>("save_emoji_favorite_data_url", { dataUrl });
        } catch (e) {
          console.warn("Failed to save dropped data url:", e);
          return null;
        }
      })
    );
    const valid = saved.filter((p): p is string => typeof p === "string" && p.length > 0);
    if (valid.length > 0) {
      persistFavorites((prev) => Array.from(new Set([...prev, ...valid])));
    }
    return valid.length;
  };

  const addFavoriteUrls = async (urls: string[]) => {
    const normalized = urls
      .map((url) => url.trim())
      .filter((url) => url.startsWith("http://") || url.startsWith("https://"));
    if (normalized.length === 0) return 0;
    const saved = await Promise.all(
      normalized.map(async (url) => {
        try {
          return await invoke<string>("save_emoji_favorite_url", { url });
        } catch (e) {
          console.warn("Failed to save emoji favorite url:", e);
          return null;
        }
      })
    );
    const valid = saved.filter((p): p is string => typeof p === "string" && p.length > 0);
    if (valid.length > 0) {
      persistFavorites((prev) => Array.from(new Set([...prev, ...valid])));
    }
    return valid.length;
  };

  const handleSend = async (content: string, contentType: string) => {
    if (contentType === "text") {
      await invoke("paste_text_directly", { content });
      return;
    }

    if (contentType === "image") {
      await invoke("paste_content_transiently", {
        content,
        contentType,
        id: 0,
        pasteWithFormat: false
      });
      return;
    }

    await invoke("copy_to_clipboard", {
      content,
      contentType,
      paste: true,
      id: 0,
      deleteAfterUse: false,
      pasteWithFormat: false
    });
  };

  const handleTabChange = (tab: "emoji" | "favorites") => {
    setActiveTab(tab);
    saveSetting("app.emoji_panel_tab", tab);
  };

  const handleSelectFiles = async () => {
    const selected = await open({
      multiple: true,
      filters: [{ name: "Images", extensions: ["png", "jpg", "jpeg", "webp", "gif", "avif"] }]
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    void addFavoritePaths(paths);
  };

  const getFilesFromDataTransfer = (dt: DataTransfer | null): File[] => {
    if (!dt) return [];
    const files: File[] = [];
    if (dt.items) {
      for (let i = 0; i < dt.items.length; i++) {
        const item = dt.items[i];
        if (item.kind === "file") {
          const file = item.getAsFile();
          if (file) {
            // In Tauri, even with dragDropEnabled: false, standard File objects dropped
            // often contain the "path" property.
            files.push(file);
          }
        }
      }
    } else if (dt.files && dt.files.length > 0) {
      return Array.from(dt.files);
    }
    return files;
  };

  const handleDomDropDataTransfer = async (dt: DataTransfer | null) => {
    let addedCount = 0;
    const files = getFilesFromDataTransfer(dt);
    if (files.length > 0) {
      addedCount += await addFavoriteFiles(files);
    }
    const candidates = await getDropCandidates(dt);
    if (candidates.length === 0) {
      return addedCount;
    }
    const localPaths = candidates
      .map((value) => normalizeDroppedLocalPath(value))
      .filter((value): value is string => !!value);
    const dataUrls = candidates.filter((url) => url.startsWith("data:"));
    const httpUrls = candidates
      .map((url) => normalizeRemoteDropUrl(url) || url)
      .filter((url) => url.startsWith("http://") || url.startsWith("https://"));

    if (addedCount === 0 && localPaths.length > 0) {
      addedCount += await addFavoritePaths(localPaths);
    }
    if (addedCount === 0 && dataUrls.length > 0) {
      addedCount += await addFavoriteDataUrls(dataUrls);
    }
    if (addedCount === 0 && httpUrls.length > 0) {
      addedCount += await addFavoriteUrls(httpUrls);
    }
    if (addedCount === 0) {
      await emit("toast", t("emoji_drop_failed") || "未识别到可添加的图片，请尝试拖拽图片本身或先下载到本地");
    }
    return addedCount;
  };

  const handleDomDrop = async (event: DragEvent<HTMLDivElement>) => {
    await handleDomDropDataTransfer(event.dataTransfer);
  };

  useEffect(() => {
    let alive = true;
    fetch("/emoji-data.json")
      .then((res) => (res.ok ? res.json() : Promise.reject(new Error("Failed to load emoji data"))))
      .then((data: EmojiData) => {
        if (!alive) return;
        const groups = Array.isArray(data?.groups) ? data.groups.filter((g) => g && Array.isArray(g.emojis)) : [];
        if (groups.length > 0) {
          setEmojiGroups(groups);
        }
      })
      .catch(() => {
        if (alive) setEmojiGroups(FALLBACK_GROUPS);
      });

    return () => {
      alive = false;
    };
  }, []);

  useEffect(() => {
    const appWindow = getCurrentWindow();

    const unlistenDrop = appWindow.listen("tauri://file-drop", (e) => {
      const paths = resolveDropPaths(e.payload);
      if (paths.length > 0) {
        void addFavoritePaths(paths);
      }
      setIsDragging(false);
    });
    const unlistenHover = appWindow.listen("tauri://file-drop-hover", () => {
      setIsDragging(true);
    });
    const unlistenCancel = appWindow.listen("tauri://file-drop-cancelled", () => {
      setIsDragging(false);
    });
    const unlistenV2Drop = appWindow.listen("tauri://drag-drop", (e) => {
      const paths = resolveDropPaths(e.payload);
      if (paths.length > 0) {
        void addFavoritePaths(paths);
      }
      setIsDragging(false);
    });
    const unlistenV2Enter = appWindow.listen("tauri://drag-enter", () => {
      setIsDragging(true);
    });
    const unlistenV2Leave = appWindow.listen("tauri://drag-leave", () => {
      setIsDragging(false);
    });
    const unlistenNativeEmoji = appWindow.listen("emoji-favorite-drop", (e) => {
      const payload = e.payload as unknown;
      const paths = resolveDropPaths(payload);
      if (paths.length === 0) return;
      const alreadySaved =
        typeof payload === "object" &&
        payload !== null &&
        "alreadySaved" in payload &&
        Boolean((payload as { alreadySaved?: boolean }).alreadySaved);
      if (alreadySaved) {
        persistFavorites((prev) => Array.from(new Set([...prev, ...paths])));
      } else {
        void addFavoritePaths(paths);
      }
      setIsDragging(false);
    });

    return () => {
      unlistenDrop.then((f) => f());
      unlistenHover.then((f) => f());
      unlistenCancel.then((f) => f());
      unlistenV2Drop.then((f) => f());
      unlistenV2Enter.then((f) => f());
      unlistenV2Leave.then((f) => f());
      unlistenNativeEmoji.then((f) => f());
    };
  }, [favorites]);

  useEffect(() => {
    const handleDragOver = (event: globalThis.DragEvent) => {
      event.preventDefault();
      if (event.dataTransfer) {
        event.dataTransfer.dropEffect = "copy";
      }
      if (!isDragging) {
        setIsDragging(true);
      }
    };

    const handleDragLeave = (event: globalThis.DragEvent) => {
      if (event.relatedTarget === null) {
        setIsDragging(false);
      }
    };

    const handleDrop = (event: globalThis.DragEvent) => {
      event.preventDefault();
      event.stopPropagation();
      setIsDragging(false);
      void handleDomDropDataTransfer(event.dataTransfer);
    };

    window.addEventListener("dragover", handleDragOver);
    window.addEventListener("dragleave", handleDragLeave);
    window.addEventListener("drop", handleDrop);

    return () => {
      window.removeEventListener("dragover", handleDragOver);
      window.removeEventListener("dragleave", handleDragLeave);
      window.removeEventListener("drop", handleDrop);
    };
  }, [isDragging]);

  return (
    <div className="emoji-panel">
      <div className="emoji-tabs">
        <button
          className={`emoji-tab ${activeTab === "emoji" ? "active" : ""}`}
          onClick={() => handleTabChange("emoji")}
        >
          <span className="emoji-tab-text">{t("emoji_tab") || "Emoji"}</span>
          {activeTab === "emoji" && (
            <motion.div layoutId="active-indicator" className="active-tab-indicator" />
          )}
        </button>
        <button
          className={`emoji-tab ${activeTab === "favorites" ? "active" : ""}`}
          onClick={() => handleTabChange("favorites")}
        >
          <span className="emoji-tab-text">{t("emoji_favorites") || "收藏"}</span>
          {activeTab === "favorites" && (
            <motion.div layoutId="active-indicator" className="active-tab-indicator" />
          )}
        </button>
      </div>

      <div className="emoji-content-wrapper">
        <AnimatePresence mode="wait" initial={false}>
          {activeTab === "emoji" ? (
            <motion.div
              key="emoji"
              className="emoji-content"
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              transition={{ duration: 0.15 }}
            >
              {emojiGroups.map((group, groupIndex) => (
                <div key={`${group.name}-${groupIndex}`} className="emoji-group">
                  <div className="emoji-group-title">{group.name}</div>
                  <div className="emoji-grid">
                    {group.emojis.map((emoji, emojiIndex) => (
                      <motion.button
                        key={`${group.name}-${groupIndex}-${emojiIndex}-${emoji}`}
                        className="emoji-btn"
                        onClick={() => handleSend(emoji, "text")}
                        title={emoji}
                        whileHover={{ scale: 1.15 }}
                        whileTap={{ scale: 0.9 }}
                      >
                        {emoji}
                      </motion.button>
                    ))}
                  </div>
                </div>
              ))}
              {flatEmoji.length === 0 && (
                <div className="emoji-empty">{t("emoji_empty") || "暂无表情"}</div>
              )}
            </motion.div>
          ) : (
            <motion.div
              key="favorites"
              className="emoji-fav-container"
              initial={{ opacity: 0, y: 10 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -10 }}
              transition={{ duration: 0.15 }}
              onClick={() => setDeleteTarget(null)}
              onContextMenu={(e) => {
                if ((e.target as HTMLElement).closest(".emoji-fav-card")) return;
                setDeleteTarget(null);
              }}
              onDragOver={(e) => {
                e.preventDefault();
                if (!isDragging) setIsDragging(true);
              }}
              onDragLeave={(e) => {
                if (e.currentTarget.contains(e.relatedTarget as Node)) return;
                setIsDragging(false);
              }}
              onDrop={(e) => {
                e.preventDefault();
                setIsDragging(false);
                void handleDomDrop(e);
              }}
            >
              <div className="emoji-fav-grid">
                {favorites.map((path, idx) => {
                  const name = path.split(/[/\\]/).pop() || path;
                  const isDeleteVisible = deleteTarget === path;
                  return (
                    <motion.div
                      key={path}
                      layout
                      initial={{ opacity: 0, scale: 0.9 }}
                      animate={{ opacity: 1, scale: 1 }}
                      transition={{ delay: idx * 0.01 }}
                      className="emoji-fav-card"
                      data-delete-visible={isDeleteVisible}
                      onContextMenu={(e) => {
                        e.preventDefault();
                        e.stopPropagation();
                        setDeleteTarget(path);
                      }}
                    >
                      <button
                        className="emoji-fav-remove"
                        title={t("delete") || "删除"}
                        onClick={(e) => {
                          e.stopPropagation();
                          removeFavoritePath(path);
                        }}
                      >
                        <X size={14} />
                      </button>
                      <button
                        className="emoji-fav-preview"
                        title={name}
                        onClick={() => handleSend(path, "image")}
                      >
                        <img
                          src={convertFileSrc(path)}
                          alt={name}
                          onError={(e) => {
                            e.currentTarget.onerror = null;
                            removeFavoritePath(path);
                          }}
                        />
                      </button>
                    </motion.div>
                  );
                })}

                <motion.div
                  className="emoji-fav-card emoji-fav-add"
                  whileHover={{ scale: 1.02 }}
                  whileTap={{ scale: 0.98 }}
                >
                  <button
                    className="emoji-fav-add-btn"
                    onClick={(e) => {
                      e.stopPropagation();
                      void handleSelectFiles();
                    }}
                    title={t("emoji_add_files") || "添加表情"}
                  >
                    <div className="add-icon-wrapper">
                      <Plus size={22} strokeWidth={2.5} />
                    </div>
                  </button>
                </motion.div>
              </div>

              {!hasFavorites && (
                <div className="emoji-fav-empty">
                  <span>{t("emoji_fav_hint") || "点击添加按钮、或拖拽图片到这里"}</span>
                </div>
              )}
              {hasFavorites && (
                <div className="emoji-fav-tip">{t("emoji_fav_tip") || "可直接拖拽图片添加"}</div>
              )}

              <AnimatePresence>
                {isDragging && (
                  <motion.div
                    className="drop-overlay"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                  >
                    <p>{t("emoji_drop_hint") || "松开鼠标即可添加"}</p>
                  </motion.div>
                )}
              </AnimatePresence>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    </div>
  );
};

export default EmojiPanel;
