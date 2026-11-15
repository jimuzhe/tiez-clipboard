import { describe, expect, test } from "vitest";
import { syncErrorText } from "./syncError";

const strings: Record<string, string> = {
  sync_error_auth: "用户名或密码不对。",
  sync_error_busy: "网盘暂时不可用，稍后会自动重试。",
  sync_error_full: "网盘空间已满。",
  sync_error_address: "同步地址不对。",
  sync_error_unreachable: "连不上同步服务器。",
  error_kind_network: "网络错误",
};
const t = (key: string) => strings[key] ?? key;

describe("syncErrorText", () => {
  test("a rejected login names the setting to fix, not the request", () => {
    expect(syncErrorText("[webdav] [network] webdav MOVE publish failed for a -> b: HTTP 401 Unauthorized", t))
      .toBe("用户名或密码不对。 (HTTP 401)");
  });

  test("rate limits and outages say a retry is coming", () => {
    expect(syncErrorText("[network] webdav PUT x failed: HTTP 503 Service Unavailable", t))
      .toBe("网盘暂时不可用，稍后会自动重试。 (HTTP 503)");
    expect(syncErrorText("transient WebDAV status 429 Too Many Requests", t))
      .toBe("网盘暂时不可用，稍后会自动重试。 (HTTP 429)");
  });

  test("a full disk and a wrong address are told apart", () => {
    expect(syncErrorText("[network] webdav PUT x failed: HTTP 507 Insufficient Storage", t)).toBe("网盘空间已满。 (HTTP 507)");
    expect(syncErrorText("[network] webdav MKCOL failed for x: HTTP 404 Not Found", t)).toBe("同步地址不对。 (HTTP 404)");
  });

  test("a server that can't be reached", () => {
    expect(syncErrorText("[network] error sending request for url (https://dav.example.com/)", t)).toBe("连不上同步服务器。");
  });

  test("anything else is shown as it is, without the backend tag", () => {
    expect(syncErrorText("[internal] status lock poisoned", t)).toBe("status lock poisoned");
  });

  test("a bare backend tag is never shown as an empty line", () => {
    expect(syncErrorText("[network]", t)).toBe("网络错误");
  });
});
