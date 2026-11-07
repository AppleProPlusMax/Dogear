import assert from "node:assert/strict";
import { test } from "node:test";
import {
  GROUP_H,
  ITEM_H,
  LIST_PAD_BOTTOM,
  LIST_PAD_TOP,
  SEARCH_CHARS,
  layoutOffsets,
  revealScroll,
  searchText,
  textMatches,
  visibleRange,
  type RowKind,
} from "./listWindow.ts";

test("search uses only the bounded prefix and still sees ocr and source", () => {
  const marker = "UNIQUE_TAIL";
  const content = `${"a".repeat(SEARCH_CHARS)}${marker}`;
  const text = searchText(content, "识别到的订单号", "Notepad.exe");
  assert.equal(text.content.includes(marker.toLowerCase()), false);
  assert.equal(text.content.length, SEARCH_CHARS);
  assert.equal(textMatches(text, "unique_tail"), false);
  assert.equal(textMatches(text, "订单号"), true);
  assert.equal(textMatches(text, "notepad"), true);
  assert.equal(textMatches(searchText("Hello JSON", null, ""), "hello"), true);
  const middle = `${"a".repeat(100)}NEEDLE${"b".repeat(100)}`;
  assert.equal(textMatches(searchText(middle, null, ""), "needle"), true);
});

test("a query cannot match across the content and ocr boundary", () => {
  const text = searchText("foo", "bar", "");
  assert.equal(textMatches(text, "foo\nbar"), false);
  assert.equal(textMatches(text, "foobar"), false);
  assert.equal(textMatches(text, "foo"), true);
  assert.equal(textMatches(text, "bar"), true);
});

test("layout height matches fixed row sizes", () => {
  const kinds: RowKind[] = ["group", "item", "item", "group", "item"];
  const layout = layoutOffsets(kinds);
  assert.equal(layout.offsets[0], LIST_PAD_TOP);
  assert.equal(layout.offsets[1], LIST_PAD_TOP + GROUP_H);
  assert.equal(layout.offsets[2], LIST_PAD_TOP + GROUP_H + ITEM_H);
  assert.equal(
    layout.total,
    LIST_PAD_TOP + GROUP_H * 2 + ITEM_H * 3 + LIST_PAD_BOTTOM,
  );
});

test("visible range covers the viewport and stays inside the list", () => {
  const kinds: RowKind[] = ["group", ...Array<RowKind>(30).fill("item")];
  const layout = layoutOffsets(kinds);
  const top = visibleRange(layout.offsets, kinds, 0, 120, 0);
  assert.equal(top.start, 0);
  assert.ok(top.end > top.start);
  assert.ok(layout.offsets[top.end - 1] < 120);
  assert.ok(layout.offsets[top.end] >= 120);

  const mid = visibleRange(layout.offsets, kinds, 400, 100, 0);
  assert.ok(layout.offsets[mid.start] + (kinds[mid.start] === "group" ? GROUP_H : ITEM_H) > 400);
  assert.ok(mid.start > 0);

  const end = visibleRange(layout.offsets, kinds, 10_000, 100, 2);
  assert.equal(end.end, kinds.length);
  assert.ok(end.start >= kinds.length - 1 - 2);
});

test("reveal scroll only moves when the row is outside the viewport", () => {
  assert.equal(revealScroll(6, ITEM_H, 0, 400), 0);
  assert.equal(revealScroll(500, ITEM_H, 0, 400), 500 + ITEM_H - 400);
  assert.equal(revealScroll(6, ITEM_H, 200, 400), 6);
  assert.equal(revealScroll(220, ITEM_H, 200, 400), 200);
});
