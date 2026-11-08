import { textMatches, type SearchText } from "./listWindow.ts";

export type ParsedSearch = {
  language: string;
  ocrOnly: boolean;
  pinnedOnly: boolean;
  text: string;
};

export type SearchClip = {
  kind?: string;
  language: string | null;
  pinned: boolean;
  ocrText: string | null;
};

const TYPE_ALIASES: Record<string, string> = {
  javascript: "js",
  typescript: "ts",
  py: "python",
  sh: "shell",
  bash: "shell",
  zsh: "shell",
  "c++": "cpp",
  cxx: "cpp",
  htm: "html",
  url: "link",
  img: "image",
  pic: "image",
  photo: "image",
  txt: "text",
  plain: "text",
};

function normalizeType(raw: string): string {
  const lower = raw.toLowerCase();
  return TYPE_ALIASES[lower] ?? lower;
}

/** 前缀只在整段搜索的开头生效。type:json 后面还可以跟要找的词。 */
export function parseSearch(raw: string): ParsedSearch {
  const trimmed = raw.trim();
  const lower = trimmed.toLowerCase();
  if (lower.startsWith("type:")) return parseType(trimmed.slice(5));
  if (lower.startsWith("ocr:")) {
    return { language: "", ocrOnly: true, pinnedOnly: false, text: trimmed.slice(4).trim().toLowerCase() };
  }
  if (lower.startsWith("pin:")) {
    return { language: "", ocrOnly: false, pinnedOnly: true, text: trimmed.slice(4).trim().toLowerCase() };
  }
  return { language: "", ocrOnly: false, pinnedOnly: false, text: lower };
}

function parseType(rest: string): ParsedSearch {
  const parts = rest.trim().split(/\s+/).filter(Boolean);
  return {
    language: normalizeType(parts[0] ?? ""),
    ocrOnly: false,
    pinnedOnly: false,
    text: parts.slice(1).join(" ").toLowerCase(),
  };
}

export function clipMatches(clip: SearchClip, parsed: ParsedSearch, text: SearchText): boolean {
  if (parsed.language) {
    const lang = (clip.language ?? "").toLowerCase();
    const kind = (clip.kind ?? "").toLowerCase();
    if (lang !== parsed.language && kind !== parsed.language) return false;
  }
  if (parsed.pinnedOnly && !clip.pinned) return false;
  if (parsed.ocrOnly) {
    if (!parsed.text) return (clip.ocrText ?? "").trim().length > 0;
    return text.ocr.includes(parsed.text);
  }
  if (!parsed.text) return true;
  return textMatches(text, parsed.text);
}
