import test from "node:test";
import assert from "node:assert/strict";
import { needsAnswerLabel, rowClick } from "../src/lib/asks.ts";

test("the label counts the questions awaiting an answer", () => {
  assert.equal(needsAnswerLabel(1), "1 question needs your answer");
  assert.equal(needsAnswerLabel(2), "2 questions need your answer");
});

test("a click on a chat with a pending question shows its oldest card", () => {
  assert.deepEqual(rowClick({ needs_answer: 2, first_question: "q1" }, true), { to: "question", id: "q1" });
});

test("a click opens the chat plainly when nothing is pending, no card is known or the screen cannot show it", () => {
  assert.deepEqual(rowClick({ needs_answer: 0, first_question: null }, true), { to: "open" });
  assert.deepEqual(rowClick({ needs_answer: 1, first_question: null }, true), { to: "open" });
  assert.deepEqual(rowClick({ needs_answer: 1 }, true), { to: "open" });
  assert.deepEqual(rowClick({ needs_answer: 1, first_question: "q1" }, false), { to: "open" });
});
