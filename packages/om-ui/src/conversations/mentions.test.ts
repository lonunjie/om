import { describe, expect, it } from "vitest";
import { activeMentionQuery, insertMention, matchCandidates, mentionRanges, textWithMentions } from "./mentions";

describe("mention helpers", () => {
  it("finds the @word under the caret only where a word starts", () => {
    expect(activeMentionQuery("hi @ma", 6)).toEqual({ from: 3, query: "ma" });
    expect(activeMentionQuery("@", 1)).toEqual({ from: 0, query: "" });
    expect(activeMentionQuery("mail me@home", 12)).toBeNull();
    expect(activeMentionQuery("hi @ma there", 12)).toBeNull();
  });
  it("matches names by contains, best prefix first, at most six", () => {
    const all = ["Ann", "Joanna", "Anders", "Bo", "Dan", "Hannah", "Zed", "Anna"].map((name) => ({ id: name, name }));
    expect(matchCandidates(all, "an").map((c) => c.name)).toEqual(["Anders", "Ann", "Anna", "Dan", "Hannah", "Joanna"]);
    expect(matchCandidates(all, "zzz")).toEqual([]);
  });
  it("inserts the label and a space and moves the caret after it", () => {
    expect(insertMention("hi @ma", 3, 6, "Maria")).toEqual({ text: "hi @Maria ", caret: 10, label: "@Maria" });
  });
  it("finds each pick in the sent text in code points, at word boundaries, and drops one that was broken", () => {
    const picks = [{ account: "a".repeat(64), label: "@Zoë" }, { account: "b".repeat(64), label: "@Bo" }];
    expect(mentionRanges("😀 @Zoë and @Bo!", picks)).toEqual([{ start: 2, end: 6, account: "a".repeat(64) }, { start: 11, end: 14, account: "b".repeat(64) }]);
    expect(mentionRanges("@Zoëlle and @Bob", picks)).toEqual([]);
    expect(mentionRanges("hi", picks)).toEqual([]);
  });
  it("builds a rich text with a mention node per range and marks the one naming us", () => {
    const doc = textWithMentions("hi @Bo!", [{ start: 3, end: 6, account: "b".repeat(64), me: true }]);
    expect(doc.blocks[0]?.children).toEqual([{ type: "text", text: "hi " }, { type: "mention", label: "Bo", personId: "b".repeat(64), me: true }, { type: "text", text: "!" }]);
    expect(textWithMentions("plain", [{ start: 9, end: 12 }]).blocks[0]?.children).toEqual([{ type: "text", text: "plain" }]);
  });
});
