import { SEARCH_CHARS } from "./listWindow.ts";

export type Tone = "" | "key" | "str" | "num" | "bool" | "kw" | "note" | "attr";

export type Piece = {
  text: string;
  hit: boolean;
  tone: Tone;
};

export type PaintMode = "plain" | "json" | "sql" | "js" | "ts" | "java" | "python" | "html" | "css" | "c" | "cpp";

const SQL_WORDS = new Set([
  "select",
  "insert",
  "update",
  "delete",
  "with",
  "create",
  "alter",
  "drop",
  "from",
  "where",
  "having",
  "limit",
  "offset",
  "values",
  "join",
  "union",
  "set",
  "and",
  "or",
  "into",
  "table",
  "group",
  "order",
  "by",
  "left",
  "right",
  "inner",
  "outer",
  "cross",
  "full",
]);

export function paint(text: string, mode: PaintMode, queryLower: string): Piece[] {
  const color = mode !== "plain" && text.length > 0 && text.length <= SEARCH_CHARS;
  const base = color ? colorize(text, mode) : [plain(text)];
  if (!queryLower) return base.filter((part) => part.text);
  const out: Piece[] = [];
  let seen = 0;
  for (const part of base) {
    const start = seen;
    seen += part.text.length;
    if (start >= SEARCH_CHARS) {
      out.push(plain(part.text));
      continue;
    }
    const room = SEARCH_CHARS - start;
    const head = part.text.slice(0, room);
    const tail = part.text.slice(room);
    out.push(...mark(head, queryLower, part.tone));
    if (tail) out.push(plain(tail));
  }
  return out.filter((part) => part.text);
}

function plain(text: string): Piece {
  return { text, hit: false, tone: "" };
}

function mark(text: string, queryLower: string, tone: Tone): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  while (index < text.length) {
    const found = findFold(text, queryLower, index);
    if (found < 0) {
      out.push({ text: text.slice(index), hit: false, tone });
      break;
    }
    if (found > index) out.push({ text: text.slice(index, found), hit: false, tone });
    out.push({ text: text.slice(found, found + queryLower.length), hit: true, tone });
    index = found + queryLower.length;
  }
  return out;
}

function findFold(text: string, queryLower: string, from: number): number {
  const last = text.length - queryLower.length;
  for (let index = from; index <= last; index += 1) {
    if (text.slice(index, index + queryLower.length).toLowerCase() === queryLower) return index;
  }
  return -1;
}

export function colorizeJson(text: string): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  let plainAt = 0;
  const flush = (end: number) => {
    if (end > plainAt) out.push(plain(text.slice(plainAt, end)));
    plainAt = end;
  };
  while (index < text.length) {
    const ch = text[index];
    if (ch === '"') {
      const start = index;
      index += 1;
      while (index < text.length) {
        if (text[index] === "\\") {
          index += 2;
          continue;
        }
        if (text[index] === '"') {
          index += 1;
          break;
        }
        index += 1;
      }
      let look = index;
      while (look < text.length && text[look] <= " ") look += 1;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: text[look] === ":" ? "key" : "str" });
      plainAt = index;
      continue;
    }
    if (ch === "-" || (ch >= "0" && ch <= "9")) {
      const start = index;
      index += 1;
      while (index < text.length && isNumberChar(text[index])) index += 1;
      if (text.slice(start, index) === "-") continue;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "num" });
      plainAt = index;
      continue;
    }
    const word = jsonWord(text, index);
    if (word) {
      flush(index);
      out.push({ text: word, hit: false, tone: "bool" });
      index += word.length;
      plainAt = index;
      continue;
    }
    index += 1;
  }
  flush(text.length);
  return out;
}

function jsonWord(text: string, index: number): string {
  for (const word of ["true", "false", "null"]) {
    if (!text.startsWith(word, index)) continue;
    const prev = index > 0 ? text[index - 1] : "";
    const next = text[index + word.length] ?? "";
    if (isIdent(prev) || isIdent(next)) continue;
    return word;
  }
  return "";
}

export function colorizeSql(text: string): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  let plainAt = 0;
  const flush = (end: number) => {
    if (end > plainAt) out.push(plain(text.slice(plainAt, end)));
    plainAt = end;
  };
  while (index < text.length) {
    const ch = text[index];
    if (ch === "'" || ch === '"' || ch === "`") {
      const quote = ch;
      index += 1;
      while (index < text.length && text[index] !== quote) index += 1;
      if (index < text.length) index += 1;
      continue;
    }
    if (ch === "-" && text[index + 1] === "-") {
      while (index < text.length && text[index] !== "\n") index += 1;
      continue;
    }
    if (ch === "/" && text[index + 1] === "*") {
      index += 2;
      while (index < text.length && !(text[index] === "*" && text[index + 1] === "/")) index += 1;
      if (index < text.length) index += 2;
      continue;
    }
    if (isIdent(ch)) {
      const start = index;
      index += 1;
      while (index < text.length && isIdent(text[index])) index += 1;
      const word = text.slice(start, index);
      if (SQL_WORDS.has(word.toLowerCase())) {
        flush(start);
        out.push({ text: word, hit: false, tone: "kw" });
        plainAt = index;
      }
      continue;
    }
    index += 1;
  }
  flush(text.length);
  return out;
}

function isNumberChar(ch: string): boolean {
  return (ch >= "0" && ch <= "9") || ch === "." || ch === "e" || ch === "E" || ch === "+" || ch === "-";
}

function isIdent(ch: string): boolean {
  return (ch >= "A" && ch <= "Z") || (ch >= "a" && ch <= "z") || (ch >= "0" && ch <= "9") || ch === "_";
}

const JS_WORDS = words(
  "await async break case catch class const continue debugger default delete do else export extends finally for from function if import in instanceof let new of return static super switch this throw try typeof var void while with yield true false null undefined",
);
const TS_WORDS = words(
  `${[...JS_WORDS].join(" ")} interface type implements public private protected readonly enum namespace declare abstract as satisfies infer keyof`,
);
const JAVA_WORDS = words(
  "abstract assert boolean break byte case catch char class const continue default do double else enum extends final finally float for goto if implements import instanceof int interface long native new package private protected public return short static strictfp super switch synchronized this throw throws transient try void volatile while true false null",
);
const C_WORDS = words(
  "auto break case char const continue default do double else enum extern float for goto if include define ifdef ifndef elif endif undef pragma inline int long register restrict return short signed sizeof static struct switch typedef union unsigned void volatile while true false NULL",
);
const CPP_WORDS = words(
  `${[...C_WORDS].join(" ")} alignas alignof and and_eq asm bitand bitor bool catch class compl constexpr const_cast decltype delete dynamic_cast explicit export friend mutable namespace new noexcept not not_eq nullptr operator or or_eq private protected public reinterpret_cast static_assert static_cast template this thread_local throw try typename using virtual wchar_t xor xor_eq cout cin endl`,
);
const PYTHON_WORDS = words(
  "and as assert async await break class continue def del elif else except finally for from global if import in is lambda nonlocal not or pass raise return try while with yield True False None",
);

function words(list: string): Set<string> {
  return new Set(list.split(/\s+/).filter(Boolean));
}

function colorize(text: string, mode: PaintMode): Piece[] {
  switch (mode) {
    case "json":
      return colorizeJson(text);
    case "sql":
      return colorizeSql(text);
    case "html":
      return colorizeHtml(text);
    case "css":
      return colorizeCss(text);
    case "python":
      return colorizeFamily(text, PYTHON_WORDS, { hash: "comment", template: false });
    case "js":
      return colorizeFamily(text, JS_WORDS, { hash: "none", template: true });
    case "ts":
      return colorizeFamily(text, TS_WORDS, { hash: "none", template: true });
    case "java":
      return colorizeFamily(text, JAVA_WORDS, { hash: "none", template: false });
    case "c":
      return colorizeFamily(text, C_WORDS, { hash: "directive", template: false });
    case "cpp":
      return colorizeFamily(text, CPP_WORDS, { hash: "directive", template: false });
    default:
      return [plain(text)];
  }
}

function colorizeFamily(
  text: string,
  keywords: Set<string>,
  options: { hash: "none" | "comment" | "directive"; template: boolean },
): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  let plainAt = 0;
  const flush = (end: number) => {
    if (end > plainAt) out.push(plain(text.slice(plainAt, end)));
    plainAt = end;
  };
  while (index < text.length) {
    const ch = text[index];
    if (options.hash === "comment" && ch === "#") {
      const start = index;
      while (index < text.length && text[index] !== "\n") index += 1;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "note" });
      plainAt = index;
      continue;
    }
    if (options.hash === "directive" && ch === "#") {
      flush(index);
      out.push({ text: "#", hit: false, tone: "kw" });
      index += 1;
      plainAt = index;
      continue;
    }
    if (ch === "/" && text[index + 1] === "/") {
      const start = index;
      while (index < text.length && text[index] !== "\n") index += 1;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "note" });
      plainAt = index;
      continue;
    }
    if (ch === "/" && text[index + 1] === "*") {
      const start = index;
      index += 2;
      while (index < text.length && !(text[index] === "*" && text[index + 1] === "/")) index += 1;
      if (index < text.length) index += 2;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "note" });
      plainAt = index;
      continue;
    }
    const prefix = options.hash === "comment" ? quotePrefix(text, index) : 0;
    const quoteAt = index + prefix;
    const quote = text[quoteAt];
    if (prefix > 0 || quote === "'" || quote === '"') {
      if (quote === "'" || quote === '"') {
        const start = index;
        index = consumeQuoted(text, quoteAt, quote);
        flush(start);
        out.push({ text: text.slice(start, index), hit: false, tone: "str" });
        plainAt = index;
        continue;
      }
    }
    if (options.template && ch === "`") {
      const start = index;
      index = consumeQuoted(text, index, "`");
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "str" });
      plainAt = index;
      continue;
    }
    if (ch >= "0" && ch <= "9" && !isIdent(index > 0 ? text[index - 1] : "")) {
      const start = index;
      index = consumeNumber(text, index);
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "num" });
      plainAt = index;
      continue;
    }
    if (isIdent(ch)) {
      const start = index;
      index += 1;
      while (index < text.length && isIdent(text[index])) index += 1;
      const word = text.slice(start, index);
      if (keywords.has(word)) {
        flush(start);
        out.push({ text: word, hit: false, tone: "kw" });
        plainAt = index;
      }
      continue;
    }
    index += 1;
  }
  flush(text.length);
  return out;
}

function quotePrefix(text: string, index: number): number {
  const sample = text.slice(index, index + 3).toLowerCase();
  for (const prefix of ["rf", "fr", "rb", "br", "f", "r", "b", "u"]) {
    if (!sample.startsWith(prefix)) continue;
    const mark = text[index + prefix.length];
    if (mark === "'" || mark === '"') return prefix.length;
  }
  return 0;
}

function consumeQuoted(text: string, index: number, quote: string): number {
  const triple = (quote === "'" || quote === '"') && text.startsWith(quote.repeat(3), index);
  const closer = triple ? quote.repeat(3) : quote;
  let cursor = index + closer.length;
  while (cursor < text.length) {
    if (text[cursor] === "\\") {
      cursor += 2;
      continue;
    }
    if (text.startsWith(closer, cursor)) return cursor + closer.length;
    cursor += 1;
  }
  return text.length;
}

function consumeNumber(text: string, index: number): number {
  let cursor = index;
  if (text[cursor] === "0" && (text[cursor + 1] === "x" || text[cursor + 1] === "X")) {
    cursor += 2;
    while (cursor < text.length && isHex(text[cursor])) cursor += 1;
    return cursor;
  }
  while (cursor < text.length && isNumberChar(text[cursor])) cursor += 1;
  return cursor;
}

function colorizeCss(text: string): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  let plainAt = 0;
  let depth = 0;
  const flush = (end: number) => {
    if (end > plainAt) out.push(plain(text.slice(plainAt, end)));
    plainAt = end;
  };
  while (index < text.length) {
    const ch = text[index];
    if (ch === "/" && text[index + 1] === "*") {
      const start = index;
      index += 2;
      while (index < text.length && !(text[index] === "*" && text[index + 1] === "/")) index += 1;
      if (index < text.length) index += 2;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "note" });
      plainAt = index;
      continue;
    }
    if (ch === "'" || ch === '"') {
      const start = index;
      index = consumeQuoted(text, index, ch);
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "str" });
      plainAt = index;
      continue;
    }
    if (ch === "@") {
      const start = index;
      index += 1;
      while (index < text.length && (isIdent(text[index]) || text[index] === "-")) index += 1;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "kw" });
      plainAt = index;
      continue;
    }
    if (ch === "!") {
      let look = index + 1;
      while (look < text.length && text[look] <= " ") look += 1;
      if (text.startsWith("important", look) && !isIdent(text[look + 9] ?? "")) {
        const start = index;
        index = look + "important".length;
        flush(start);
        out.push({ text: text.slice(start, index), hit: false, tone: "kw" });
        plainAt = index;
        continue;
      }
    }
    if (ch === "#") {
      const end = hexColorEnd(text, index + 1);
      if (end > index + 1) {
        const start = index;
        index = end;
        flush(start);
        out.push({ text: text.slice(start, index), hit: false, tone: "num" });
        plainAt = index;
        continue;
      }
    }
    if (isCssNumberStart(text, index)) {
      const start = index;
      index = consumeCssNumber(text, index);
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "num" });
      plainAt = index;
      continue;
    }
    if (isIdent(ch) || (ch === "-" && isIdent(text[index + 1] ?? ""))) {
      const start = index;
      index += 1;
      while (index < text.length && (isIdent(text[index]) || text[index] === "-")) index += 1;
      let look = index;
      while (look < text.length && text[look] <= " ") look += 1;
      const property = depth > 0 && text[look] === ":";
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: property ? "attr" : "key" });
      plainAt = index;
      continue;
    }
    if (ch === "{") depth += 1;
    if (ch === "}") depth = Math.max(0, depth - 1);
    index += 1;
  }
  flush(text.length);
  return out;
}

function hexColorEnd(text: string, index: number): number {
  let cursor = index;
  while (cursor < text.length && isHex(text[cursor])) cursor += 1;
  const length = cursor - index;
  if ((length === 3 || length === 4 || length === 6 || length === 8) && !isIdent(text[cursor] ?? "")) return cursor;
  return index;
}

function isCssNumberStart(text: string, index: number): boolean {
  const ch = text[index];
  if (ch >= "0" && ch <= "9") return !isIdent(index > 0 ? text[index - 1] : "");
  return ch === "." && (text[index + 1] ?? "") >= "0" && (text[index + 1] ?? "") <= "9";
}

function consumeCssNumber(text: string, index: number): number {
  const cursor = consumeNumber(text, index);
  if (text[cursor] === "%") return cursor + 1;
  let end = cursor;
  while (end < text.length && isIdent(text[end])) end += 1;
  const unit = text.slice(cursor, end).toLowerCase();
  if (unit && CSS_UNITS.has(unit)) return end;
  return cursor;
}

const CSS_UNITS = new Set(["px", "em", "rem", "vh", "vw", "vmin", "vmax", "ch", "ex", "pt", "pc", "cm", "mm", "in", "deg", "rad", "turn", "s", "ms", "fr"]);

function colorizeHtml(text: string): Piece[] {
  const out: Piece[] = [];
  let index = 0;
  let plainAt = 0;
  const flush = (end: number) => {
    if (end > plainAt) out.push(plain(text.slice(plainAt, end)));
    plainAt = end;
  };
  while (index < text.length) {
    if (text.startsWith("<!--", index)) {
      const start = index;
      const end = text.indexOf("-->", index + 4);
      index = end < 0 ? text.length : end + 3;
      flush(start);
      out.push({ text: text.slice(start, index), hit: false, tone: "note" });
      plainAt = index;
      continue;
    }
    if (text[index] === "<" && isTagStart(text, index + 1)) {
      flush(index);
      index += 1;
      if (text[index] === "/") index += 1;
      let named = false;
      while (index < text.length && text[index] !== ">") {
        const ch = text[index];
        if (ch === '"' || ch === "'") {
          const start = index;
          index = consumeQuoted(text, index, ch);
          flush(start);
          out.push({ text: text.slice(start, index), hit: false, tone: "str" });
          plainAt = index;
          continue;
        }
        if (isTagNameChar(ch)) {
          const start = index;
          while (index < text.length && isTagNameChar(text[index])) index += 1;
          flush(start);
          out.push({ text: text.slice(start, index), hit: false, tone: named ? "attr" : "key" });
          named = true;
          plainAt = index;
          continue;
        }
        index += 1;
      }
      if (index < text.length && text[index] === ">") index += 1;
      continue;
    }
    index += 1;
  }
  flush(text.length);
  return out;
}

function isTagStart(text: string, index: number): boolean {
  const ch = text[index] ?? "";
  if (ch === "/") return isTagNameChar(text[index + 1] ?? "");
  return isTagNameChar(ch) || ch === "!" || ch === "?";
}

function isTagNameChar(ch: string): boolean {
  return isIdent(ch) || ch === "-" || ch === ":";
}

function isHex(ch: string): boolean {
  return (ch >= "0" && ch <= "9") || (ch >= "a" && ch <= "f") || (ch >= "A" && ch <= "F");
}
