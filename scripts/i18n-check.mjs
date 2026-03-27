import { promises as fs } from "node:fs";
import path from "node:path";

const rootDir = process.cwd();
const srcDir = path.join(rootDir, "src");
const localePaths = {
  zh: path.join(srcDir, "locales", "zh.ts"),
  en: path.join(srcDir, "locales", "en.ts"),
  tw: path.join(srcDir, "locales", "tw.ts")
};

const sourceExtensions = new Set([".ts", ".tsx"]);

const collectFiles = async (dir) => {
  const entries = await fs.readdir(dir, { withFileTypes: true });
  const files = [];
  for (const entry of entries) {
    if (entry.name === "node_modules" || entry.name === "dist" || entry.name === "target") {
      continue;
    }
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      files.push(...(await collectFiles(fullPath)));
      continue;
    }
    if (sourceExtensions.has(path.extname(entry.name))) {
      files.push(fullPath);
    }
  }
  return files;
};

const extractLocaleKeys = (content) => {
  const keys = new Set();
  const keyRegex = /^\s*([A-Za-z0-9_]+)\s*:/gm;
  let match = keyRegex.exec(content);
  while (match) {
    keys.add(match[1]);
    match = keyRegex.exec(content);
  }
  return keys;
};

const escapeRegex = (value) => value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

const templateToRegex = (template) => {
  const placeholderToken = "__I18N_PLACEHOLDER__";
  const withToken = template.replace(/\$\{[^}]+\}/g, placeholderToken);
  const escaped = escapeRegex(withToken);
  const pattern = escaped.replaceAll(placeholderToken, "[A-Za-z0-9_]+");
  return new RegExp(`^${pattern}$`);
};

const extractUsedKeys = (content) => {
  const keys = new Set();
  const templatePatterns = [];
  const templateVariablePatterns = new Map();
  const literalVariableValues = new Map();
  const literalRegexes = [
    /\b(?:props\.)?t\(\s*"([^"\n]+)"/g,
    /\b(?:props\.)?t\(\s*'([^'\n]+)'/g,
    /\b(?:props\.)?t\(\s*`([^`\n]+)`/g
  ];
  for (const regex of literalRegexes) {
    let match = regex.exec(content);
    while (match) {
      const value = match[1].trim();
      if (value.includes("${")) {
        templatePatterns.push(templateToRegex(value));
      } else {
        keys.add(value);
      }
      match = regex.exec(content);
    }
  }
  const variableTemplateRegex = /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*`([^`\n]+)`\s*;?/g;
  let variableTemplateMatch = variableTemplateRegex.exec(content);
  while (variableTemplateMatch) {
    const variableName = variableTemplateMatch[1];
    const templateValue = variableTemplateMatch[2].trim();
    if (templateValue.includes("${")) {
      templateVariablePatterns.set(variableName, templateToRegex(templateValue));
    } else if (templateValue) {
      literalVariableValues.set(variableName, templateValue);
    }
    variableTemplateMatch = variableTemplateRegex.exec(content);
  }
  const variableStringRegex = /\b(?:const|let|var)\s+([A-Za-z_$][\w$]*)\s*=\s*(?:"([^"\n]+)"|'([^'\n]+)')\s*;?/g;
  let variableStringMatch = variableStringRegex.exec(content);
  while (variableStringMatch) {
    const variableName = variableStringMatch[1];
    const value = (variableStringMatch[2] ?? variableStringMatch[3] ?? "").trim();
    if (value) {
      literalVariableValues.set(variableName, value);
    }
    variableStringMatch = variableStringRegex.exec(content);
  }
  const variableCallRegex = /\b(?:props\.)?t\(\s*([A-Za-z_$][\w$]*)\s*\)/g;
  let variableCallMatch = variableCallRegex.exec(content);
  while (variableCallMatch) {
    const variableName = variableCallMatch[1];
    const pattern = templateVariablePatterns.get(variableName);
    if (pattern) {
      templatePatterns.push(pattern);
    }
    const value = literalVariableValues.get(variableName);
    if (value) {
      keys.add(value);
    }
    variableCallMatch = variableCallRegex.exec(content);
  }
  return { keys, templatePatterns };
};

const difference = (left, right) => [...left].filter((key) => !right.has(key)).sort();

const readLocaleKeyMap = async () => {
  const entries = await Promise.all(
    Object.entries(localePaths).map(async ([locale, filePath]) => {
      const content = await fs.readFile(filePath, "utf8");
      return [locale, extractLocaleKeys(content)];
    })
  );
  return new Map(entries);
};

const run = async () => {
  const localeKeyMap = await readLocaleKeyMap();
  const baseKeys = localeKeyMap.get("zh");
  if (!baseKeys) {
    throw new Error("找不到 zh 语言 key 集合");
  }

  const files = await collectFiles(srcDir);
  const usedKeys = new Set();
  const templatePatterns = [];

  for (const file of files) {
    const content = await fs.readFile(file, "utf8");
    const extracted = extractUsedKeys(content);
    for (const key of extracted.keys) {
      usedKeys.add(key);
    }
    templatePatterns.push(...extracted.templatePatterns);
  }

  for (const pattern of templatePatterns) {
    for (const key of baseKeys) {
      if (pattern.test(key)) {
        usedKeys.add(key);
      }
    }
  }

  const unusedKeys = difference(baseKeys, usedKeys);
  const missingFromBase = difference(usedKeys, baseKeys);
  const localeDiffs = [];

  for (const [locale, keys] of localeKeyMap.entries()) {
    if (locale === "zh") continue;
    localeDiffs.push({
      locale,
      missing: difference(baseKeys, keys),
      extra: difference(keys, baseKeys)
    });
  }

  const hasIssue =
    unusedKeys.length > 0 ||
    missingFromBase.length > 0 ||
    localeDiffs.some((item) => item.missing.length > 0 || item.extra.length > 0);

  console.log("i18n key 检查结果");
  console.log(`- 基准语言(zh) keys: ${baseKeys.size}`);
  console.log(`- 代码使用 keys: ${usedKeys.size}`);
  console.log(`- 未使用 keys: ${unusedKeys.length}`);
  console.log(`- 代码缺失 keys: ${missingFromBase.length}`);

  if (unusedKeys.length > 0) {
    console.log("\n[未使用 keys]");
    console.log(unusedKeys.join("\n"));
  }

  if (missingFromBase.length > 0) {
    console.log("\n[代码引用但 zh 缺失 keys]");
    console.log(missingFromBase.join("\n"));
  }

  for (const diff of localeDiffs) {
    if (diff.missing.length > 0) {
      console.log(`\n[${diff.locale} 缺失 keys]`);
      console.log(diff.missing.join("\n"));
    }
    if (diff.extra.length > 0) {
      console.log(`\n[${diff.locale} 多余 keys]`);
      console.log(diff.extra.join("\n"));
    }
  }

  if (hasIssue) {
    process.exitCode = 1;
    return;
  }

  console.log("\n✅ i18n key 一致且无未使用项");
};

run().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
