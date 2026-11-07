/** 搜索只看每条正文和识别文字的前这么多个字符。更长的部分仍在预览里。 */
export const SEARCH_CHARS = 16_000;

export const ITEM_H = 54;
export const GROUP_H = 28;
export const LIST_PAD_TOP = 6;
export const LIST_PAD_BOTTOM = 12;
export const OVERSCAN = 8;

export type RowKind = "group" | "item";

export type SearchText = {
  content: string;
  ocr: string;
  source: string;
};

export function rowHeight(kind: RowKind): number {
  return kind === "group" ? GROUP_H : ITEM_H;
}

function bounded(text: string): string {
  return text.length <= SEARCH_CHARS ? text : text.slice(0, SEARCH_CHARS);
}

/** 三段分开保存，避免查询词正好跨在正文和识别文字的接缝上时被误匹配。 */
export function searchText(content: string, ocr: string | null | undefined, source: string): SearchText {
  return {
    content: bounded(content).toLowerCase(),
    ocr: bounded(ocr ?? "").toLowerCase(),
    source: source.toLowerCase(),
  };
}

export function textMatches(text: SearchText, queryLower: string): boolean {
  return text.content.includes(queryLower) || text.ocr.includes(queryLower) || text.source.includes(queryLower);
}

export function layoutOffsets(kinds: RowKind[]): { offsets: number[]; total: number } {
  const offsets: number[] = [];
  let y = LIST_PAD_TOP;
  for (const kind of kinds) {
    offsets.push(y);
    y += rowHeight(kind);
  }
  return { offsets, total: y + LIST_PAD_BOTTOM };
}

/** 返回半开区间 [start, end)，多画出 overscan 行，避免快滚时露白。 */
export function visibleRange(
  offsets: number[],
  kinds: RowKind[],
  scrollTop: number,
  viewHeight: number,
  overscan = OVERSCAN,
): { start: number; end: number } {
  const count = offsets.length;
  if (count === 0) return { start: 0, end: 0 };
  const startY = Math.max(0, scrollTop);
  const endY = startY + Math.max(0, viewHeight);
  let start = firstIntersecting(offsets, kinds, startY);
  let end = firstAtOrBelow(offsets, endY);
  start = Math.max(0, start - overscan);
  end = Math.min(count, end + overscan);
  if (end < start) end = start;
  return { start, end };
}

function firstIntersecting(offsets: number[], kinds: RowKind[], y: number): number {
  let lo = 0;
  let hi = offsets.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid] + rowHeight(kinds[mid]) <= y) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

function firstAtOrBelow(offsets: number[], y: number): number {
  let lo = 0;
  let hi = offsets.length;
  while (lo < hi) {
    const mid = (lo + hi) >> 1;
    if (offsets[mid] < y) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

/** 把一行滚进视口。已经看得见时保持原滚动位置。 */
export function revealScroll(itemTop: number, itemHeight: number, scrollTop: number, viewHeight: number): number {
  const top = Math.max(0, itemTop);
  const bottom = top + itemHeight;
  if (top < scrollTop) return top;
  if (bottom > scrollTop + viewHeight) return Math.max(0, bottom - viewHeight);
  return scrollTop;
}
