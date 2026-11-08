import assert from "node:assert/strict";
import { test } from "node:test";
import { searchText } from "./listWindow.ts";
import { clipMatches, parseSearch } from "./searchQuery.ts";

const clip = {
  kind: "code",
  language: "json",
  pinned: false,
  ocrText: "订单号",
};

test("prefixes apply only at the start and keep the words after them", () => {
  assert.deepEqual(parseSearch("type:json error"), {
    language: "json",
    ocrOnly: false,
    pinnedOnly: false,
    text: "error",
  });
  assert.deepEqual(parseSearch("TYPE:SQL"), {
    language: "sql",
    ocrOnly: false,
    pinnedOnly: false,
    text: "",
  });
  assert.equal(parseSearch("type:JavaScript").language, "js");
  assert.equal(parseSearch("type:C++").language, "cpp");
  assert.equal(parseSearch("type:url").language, "link");
  assert.equal(parseSearch("ocr:订单").ocrOnly, true);
  assert.equal(parseSearch("ocr:订单").text, "订单");
  assert.equal(parseSearch("pin:").pinnedOnly, true);
  assert.equal(parseSearch("pin:订单").text, "订单");
  assert.equal(parseSearch("see type:json").language, "");
  assert.equal(parseSearch("see type:json").text, "see type:json");
});

test("type, ocr, and pin narrow the match", () => {
  const text = searchText('{"error":1}', "订单号", "Notepad.exe");
  assert.equal(clipMatches(clip, parseSearch("type:json error"), text), true);
  assert.equal(clipMatches(clip, parseSearch("type:code error"), text), true);
  assert.equal(clipMatches(clip, parseSearch("type:sql error"), text), false);
  assert.equal(
    clipMatches(
      { kind: "image", language: null, pinned: false, ocrText: "订单号" },
      parseSearch("type:image 订单"),
      text,
    ),
    true,
  );
  assert.equal(clipMatches(clip, parseSearch("ocr:订单"), text), true);
  assert.equal(clipMatches(clip, parseSearch("ocr:error"), text), false);
  assert.equal(clipMatches({ ...clip, ocrText: "  " }, parseSearch("ocr:"), text), false);
  assert.equal(clipMatches(clip, parseSearch("ocr:"), text), true);
  assert.equal(clipMatches({ ...clip, pinned: true }, parseSearch("pin:error"), text), true);
  assert.equal(clipMatches(clip, parseSearch("pin:error"), text), false);
  assert.equal(clipMatches(clip, parseSearch("pin:"), text), false);
});
