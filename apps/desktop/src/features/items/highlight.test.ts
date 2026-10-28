import { describe, expect, it } from "vitest";
import { highlightSegments, matchRanges } from "./highlight";

describe("matchRanges", () => {
  it("highlights a prefix", () => {
    expect(matchRanges("GitHub Login", "git")).toEqual([{ start: 0, end: 3 }]);
  });

  it("highlights a word anywhere in the text", () => {
    expect(matchRanges("My GitHub Login", "git")).toEqual([{ start: 3, end: 6 }]);
  });

  it("highlights a multi-word substring", () => {
    expect(matchRanges("My GitHub Login", "hub log")).toEqual([
      { start: 6, end: 13 },
    ]);
  });

  it("highlights every non-overlapping occurrence", () => {
    expect(matchRanges("test test", "test")).toEqual([
      { start: 0, end: 4 },
      { start: 5, end: 9 },
    ]);
  });

  it("falls back to a subsequence when there is no contiguous match", () => {
    expect(matchRanges("GitHub Login", "ghl")).toEqual([
      { start: 0, end: 1 },
      { start: 3, end: 4 },
      { start: 7, end: 8 },
    ]);
  });

  it("merges adjacent subsequence characters", () => {
    expect(matchRanges("GitHub Login", "gitl")).toEqual([
      { start: 0, end: 3 },
      { start: 7, end: 8 },
    ]);
  });

  it("ignores case", () => {
    expect(matchRanges("GitHub", "GITHUB")).toEqual([{ start: 0, end: 6 }]);
    expect(matchRanges("github", "Git")).toEqual([{ start: 0, end: 3 }]);
  });

  it("ignores diacritics in both text and query", () => {
    expect(matchRanges("Résumé", "resume")).toEqual([{ start: 0, end: 6 }]);
    expect(matchRanges("Resume", "résumé")).toEqual([{ start: 0, end: 6 }]);
    expect(matchRanges("Café Login", "cafe")).toEqual([{ start: 0, end: 4 }]);
  });

  it("maps ranges back through decomposed characters", () => {
    // "Re\u0301sume\u0301" is 8 UTF-16 code units: the combining accents are
    // skipped while folding, but the range must still cover all of them.
    expect(matchRanges("Re\u0301sume\u0301", "resume")).toEqual([
      { start: 0, end: 8 },
    ]);
  });

  it("returns no ranges for an empty or whitespace-only query", () => {
    expect(matchRanges("GitHub", "")).toEqual([]);
    expect(matchRanges("GitHub", "   ")).toEqual([]);
    expect(matchRanges("", "git")).toEqual([]);
  });

  it("returns no ranges when the query does not match", () => {
    expect(matchRanges("GitHub", "zzz")).toEqual([]);
  });

  it("treats regex metacharacters as literal text", () => {
    expect(matchRanges("a+b (test)", "+b (")).toEqual([{ start: 1, end: 5 }]);
    expect(matchRanges("a.b", ".")).toEqual([{ start: 1, end: 2 }]);
  });

  it("caps the number of ranges", () => {
    const text = "a ".repeat(200);
    expect(matchRanges(text, "a").length).toBe(100);
  });
});

describe("highlightSegments", () => {
  it("returns one plain segment when nothing matches", () => {
    expect(highlightSegments("GitHub", "zzz")).toEqual([
      { text: "GitHub", match: false },
    ]);
  });

  it("splits text into plain and matched segments without losing characters", () => {
    const text = "My GitHub Login";
    const segments = highlightSegments(text, "git");
    expect(segments).toEqual([
      { text: "My ", match: false },
      { text: "Git", match: true },
      { text: "Hub Login", match: false },
    ]);
    expect(segments.map((s) => s.text).join("")).toBe(text);
  });

  it("marks subsequence characters separately", () => {
    const segments = highlightSegments("GitHub Login", "ghl");
    expect(segments.filter((s) => s.match).map((s) => s.text)).toEqual([
      "G",
      "H",
      "L",
    ]);
    expect(segments.map((s) => s.text).join("")).toBe("GitHub Login");
  });
});
