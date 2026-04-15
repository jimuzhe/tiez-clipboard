const MISSING_LEADING_TAG_RE =
  /^(table|tbody|thead|tfoot|tr|td|th|colgroup|col|div|span|p|ul|ol|li|blockquote|pre|h[1-6]|meta|style|img|a)\b[^>]*>/i;
const OFFICE_STYLE_SIGNAL_RE =
  /(?:\/\*\s*style definitions\s*\*\/|mso-style-name|mso-style-noshow|mso-style-priority|mso-padding-alt|mso-para-margin|table\.mso|mso-|microsoftinternetexplorer\d*|documentnotspecified|wps office|office word|msonormal|mso normal|normal\s+\d+\s+false)/i;
const RENDERABLE_CONTENT_TAG_RE =
  /<(table|p|div|span|img|a|ul|ol|li|blockquote|pre|h[1-6])\b/i;
const OFFICE_STYLE_BLOCK_RE = /<style\b[\s\S]*?<\/style>/gi;
const OFFICE_XML_BLOCK_RE = /<xml\b[\s\S]*?<\/xml>/gi;
const CONDITIONAL_COMMENT_RE = /<!--[\s\S]*?-->/gi;
const BODY_RE = /<body\b[^>]*>([\s\S]*?)<\/body\s*>/i;
const HEAD_RE = /<head\b[\s\S]*?<\/head\s*>/gi;

export const isHtmlishTagText = (text: string): boolean => {
  return MISSING_LEADING_TAG_RE.test((text || "").trim());
};

export const isOfficeStyleDefinitionText = (text: string): boolean => {
  const normalized = (text || "").replace(/\s+/g, " ").trim();
  return normalized.length > 24 && OFFICE_STYLE_SIGNAL_RE.test(normalized);
};

export const repairHtmlFragment = (html: string): string => {
  const trimmed = (html || "").trim();
  if (!trimmed || trimmed.startsWith("<")) {
    return trimmed;
  }

  if (isHtmlishTagText(trimmed)) {
    return `<${trimmed}`;
  }

  return trimmed;
};

export const extractRenderableHtmlFragment = (html: string): string => {
  const repaired = repairHtmlFragment(html || "");
  const trimmed = repaired.trim();
  if (!trimmed) {
    return trimmed;
  }

  const startMarker = "<!--StartFragment-->";
  const endMarker = "<!--EndFragment-->";
  const startIndex = trimmed.indexOf(startMarker);
  if (startIndex >= 0) {
    const contentStart = startIndex + startMarker.length;
    const endIndex = trimmed.indexOf(endMarker, contentStart);
    if (endIndex > contentStart) {
      return trimmed.slice(contentStart, endIndex).trim();
    }
  }

  const bodyMatch = trimmed.match(BODY_RE);
  if (bodyMatch?.[1]) {
    return bodyMatch[1].trim();
  }

  return trimmed.replace(HEAD_RE, " ").trim();
};

export const stripOfficePreviewNoise = (html: string): string => {
  let processed = repairHtmlFragment(html || "");
  if (!processed) {
    return processed;
  }

  processed = processed.replace(OFFICE_XML_BLOCK_RE, (block) =>
    isOfficeStyleDefinitionText(block) ? " " : block
  );
  processed = processed.replace(OFFICE_STYLE_BLOCK_RE, (block) =>
    isOfficeStyleDefinitionText(block) ? " " : block
  );
  processed = processed.replace(CONDITIONAL_COMMENT_RE, (block) =>
    isOfficeStyleDefinitionText(block) ? " " : block
  );

  const match = RENDERABLE_CONTENT_TAG_RE.exec(processed);
  if (match && match.index > 0) {
    const prefix = processed.slice(0, match.index);
    if (isOfficeStyleDefinitionText(prefix)) {
      processed = processed.slice(match.index);
    }
  }

  return processed.trim();
};
