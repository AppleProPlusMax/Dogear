import assert from "node:assert/strict";
import { test } from "node:test";
import { SEARCH_CHARS } from "./listWindow.ts";
import { colorizeJson, colorizeSql, paint } from "./richText.ts";

test("highlight keeps the original letters and ignores the tail", () => {
  const body = `${"a".repeat(SEARCH_CHARS)}NEEDLE`;
  const parts = paint(`前缀 NEEDLE 后缀`, "plain", "needle");
  assert.deepEqual(
    parts.map((part) => [part.text, part.hit]),
    [
      ["前缀 ", false],
      ["NEEDLE", true],
      [" 后缀", false],
    ],
  );
  const long = paint(body, "plain", "needle");
  assert.equal(long.some((part) => part.hit), false);
  assert.equal(long.map((part) => part.text).join(""), body);
});

test("json tokens keep the original text", () => {
  const text = '{\n  "name": "Ada",\n  "n": 2,\n  "ok": true\n}';
  const parts = colorizeJson(text);
  assert.equal(parts.map((part) => part.text).join(""), text);
  assert.equal(parts.find((part) => part.text === '"name"')?.tone, "key");
  assert.equal(parts.find((part) => part.text === '"Ada"')?.tone, "str");
  assert.equal(parts.find((part) => part.text === "2")?.tone, "num");
  assert.equal(parts.find((part) => part.text === "true")?.tone, "bool");
});

test("sql keywords stay plain inside quotes and comments", () => {
  const text = "SELECT name FROM t WHERE name = 'SELECT' -- FROM";
  const parts = colorizeSql(text);
  assert.equal(parts.map((part) => part.text).join(""), text);
  const words = parts.filter((part) => part.tone === "kw").map((part) => part.text);
  assert.deepEqual(words, ["SELECT", "FROM", "WHERE"]);
});

test("code languages color keywords, strings, and comments without rewriting the text", () => {
  const js = "const name = \"Ada\"; // keep";
  const jsParts = paint(js, "js", "");
  assert.equal(jsParts.map((part) => part.text).join(""), js);
  assert.equal(jsParts.find((part) => part.text === "const")?.tone, "kw");
  assert.equal(jsParts.find((part) => part.text === "\"Ada\"")?.tone, "str");
  assert.equal(jsParts.find((part) => part.text === "// keep")?.tone, "note");

  const py = "def hello():\n    return 'Ada'  # note";
  const pyParts = paint(py, "python", "");
  assert.equal(pyParts.map((part) => part.text).join(""), py);
  assert.equal(pyParts.find((part) => part.text === "def")?.tone, "kw");
  assert.equal(pyParts.find((part) => part.text.startsWith("#"))?.tone, "note");

  const html = '<div class="panel">Hi</div>';
  const htmlParts = paint(html, "html", "");
  assert.equal(htmlParts.map((part) => part.text).join(""), html);
  assert.equal(htmlParts.find((part) => part.text === "div")?.tone, "key");
  assert.equal(htmlParts.find((part) => part.text === "class")?.tone, "attr");
  assert.equal(htmlParts.find((part) => part.text === "\"panel\"")?.tone, "str");

  const css = ".panel {\n  color: #fff;\n  margin: 10px !important; /* note */\n}\n";
  const cssParts = paint(css, "css", "");
  assert.equal(cssParts.map((part) => part.text).join(""), css);
  assert.equal(cssParts.find((part) => part.text === "panel")?.tone, "key");
  assert.equal(cssParts.find((part) => part.text === "color")?.tone, "attr");
  assert.equal(cssParts.find((part) => part.text === "#fff")?.tone, "num");
  assert.equal(cssParts.find((part) => part.text === "10px")?.tone, "num");
  assert.equal(cssParts.find((part) => part.text === "!important")?.tone, "kw");
  assert.equal(cssParts.find((part) => part.text === "/* note */")?.tone, "note");

  const cpp = '#include <iostream>\nstd::cout << "hi"; // note';
  const cppParts = paint(cpp, "cpp", "");
  assert.equal(cppParts.map((part) => part.text).join(""), cpp);
  assert.equal(cppParts.find((part) => part.text === "include")?.tone, "kw");
  assert.equal(cppParts.find((part) => part.text === "cout")?.tone, "kw");
  assert.equal(cppParts.filter((part) => part.text === "include").every((part) => part.tone === "kw"), true);
  const quoted = cppParts.find((part) => part.text.includes("include") && part.text.includes("\""));
  assert.equal(quoted, undefined);
});
test("long json is not colored, short json can still highlight a value", () => {
  const text = `{"name":"Ada"}${" ".repeat(SEARCH_CHARS)}`;
  const plain = paint(text, "json", "ada");
  assert.equal(plain.some((part) => part.tone), false);
  const colored = paint('{"name":"Ada"}', "json", "ada");
  assert.equal(colored.find((part) => part.text === '"name"')?.tone, "key");
  assert.equal(colored.find((part) => part.text === "Ada")?.hit, true);
});
