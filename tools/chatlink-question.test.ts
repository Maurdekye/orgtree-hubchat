import test from "node:test";
import assert from "node:assert/strict";
import { chatLinkQuestion, chatLinkTarget } from "../src/lib/chatlink.ts";

test("a chat link names the question card it points at", () => {
  const link = "hubchat://chat?to=pat.4d5e6f&owner=me.000000&q=7f3c2a10-aaaa-bbbb-cccc-0123456789ab";
  assert.equal(chatLinkQuestion(link), "7f3c2a10-aaaa-bbbb-cccc-0123456789ab");
  assert.equal(chatLinkTarget(link, "me.000000"), "pat.4d5e6f", "the chat is still opened by the existing parser");
});

test("a link without a usable question has none", () => {
  assert.equal(chatLinkQuestion("hubchat://chat?to=pat.4d5e6f"), null);
  assert.equal(chatLinkQuestion("hubchat://chat?to=pat.4d5e6f&q="), null);
  assert.equal(chatLinkQuestion("hubchat://chat?to=pat.4d5e6f&q=has space"), null);
  assert.equal(chatLinkQuestion("hubchat://chat?to=pat.4d5e6f&q=" + "a".repeat(201)), null);
  assert.equal(chatLinkQuestion("hubchat://view?name=settings&q=abc"), null, "only a chat link");
  assert.equal(chatLinkQuestion("not a link"), null);
});
