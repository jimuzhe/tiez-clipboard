import { describe, it, expect } from "vitest";
import { formatSensitivePreview } from "./utils";

describe("formatSensitivePreview", () => {
  describe("空值处理", () => {
    it("空字符串返回空", () => {
      expect(formatSensitivePreview("", "text")).toBe("");
    });
  });

  describe("普通文本掩码", () => {
    it("长文本保留前3后3", () => {
      const result = formatSensitivePreview("HelloWorld123", "text");
      expect(result).toBe("Hel...123");
    });

    it("极短文本(<=2字符)全部遮蔽", () => {
      expect(formatSensitivePreview("ab", "text")).toBe("...");
      expect(formatSensitivePreview("a", "text")).toBe("...");
    });

    it("3字符文本只能露出部分", () => {
      // 3 chars, available = 3 - 2 = 1, prefix = min(3, floor(1/2)) = 0, suffix = min(3, 1-0) = 1
      const result = formatSensitivePreview("abc", "text");
      expect(result).toBe("...c");
    });

    it("自定义 prefix/suffix 可见数", () => {
      const result = formatSensitivePreview("1234567890", "text", {
        prefixVisible: 2,
        suffixVisible: 2,
      });
      expect(result).toBe("12...90");
    });

    it("prefix+suffix 超过可用字符时自动截断", () => {
      // 5 chars, available = 5 - 2 = 3, prefix = min(10, floor(3/2)) = 1, suffix = min(10, 3-1) = 2
      const result = formatSensitivePreview("abcde", "text", {
        prefixVisible: 10,
        suffixVisible: 10,
      });
      expect(result).toBe("a...de");
    });
  });

  describe("URL 掩码", () => {
    it("保留协议头，遮蔽其余部分", () => {
      const result = formatSensitivePreview(
        "https://www.example.com/path",
        "url"
      );
      expect(result).toMatch(/^https:\/\//);
      expect(result).toContain("...");
      expect(result).not.toBe("https://www.example.com/path");
    });

    it("http 协议同样保留", () => {
      const result = formatSensitivePreview(
        "http://secret-server.internal:8080/api",
        "url"
      );
      expect(result).toMatch(/^http:\/\//);
      expect(result).toContain("...");
    });

    it("自定义协议也能识别", () => {
      const result = formatSensitivePreview(
        "ftp://files.example.com/doc.pdf",
        "url"
      );
      expect(result).toMatch(/^ftp:\/\//);
      expect(result).toContain("...");
    });

    it("无协议的 URL 按普通文本掩码", () => {
      const result = formatSensitivePreview("www.example.com", "url");
      expect(result).toBe("www...com");
    });
  });

  describe("邮箱掩码", () => {
    it("默认只遮蔽本地部分，保留域名", () => {
      const result = formatSensitivePreview(
        "username@example.com",
        "text"
      );
      expect(result).toContain("@example.com");
      expect(result).toContain("...");
      expect(result).not.toContain("username");
    });

    it("开启 maskEmailDomain 时域名也遮蔽", () => {
      const result = formatSensitivePreview(
        "username@example.com",
        "text",
        { maskEmailDomain: true }
      );
      expect(result).toContain("@");
      expect(result).not.toContain("example.com");
    });

    it("带空格的邮箱也能 trim 后识别", () => {
      const result = formatSensitivePreview(
        "  user@test.org  ",
        "text"
      );
      expect(result).toContain("@test.org");
      expect(result).toContain("...");
    });

    it("短本地部分(<=2字符)全部遮蔽", () => {
      const result = formatSensitivePreview("ab@example.com", "text");
      expect(result).toBe("...@example.com");
    });
  });

  describe("Unicode 支持", () => {
    it("中文字符按字符而非字节计数", () => {
      const result = formatSensitivePreview("你好世界测试文本", "text");
      expect(result).toContain("...");
      // 8 chars, available = 6, prefix = min(3,3) = 3, suffix = min(3,3) = 3
      expect(result).toBe("你好世...试文本");
    });

    it("emoji 按字符计数", () => {
      const result = formatSensitivePreview("😀😁😂🤣😃😄😅", "text");
      expect(result).toContain("...");
    });
  });
});
