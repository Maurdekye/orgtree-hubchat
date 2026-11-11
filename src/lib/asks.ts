// A chat that has an incoming question card waiting for an answer (the taskbar dot's rule, docket
// hubchat-notification-pip-stuck-on-with-no-unread). STUB for the red commit.
export const needsAnswerLabel = (_n: number): string => "";

export type RowClick = { to: "open" } | { to: "question"; id: string };

/** What a click on a chat row does: open the chat, or open it and show its oldest pending card. */
export const rowClick = (_c: { needs_answer: number; first_question?: string | null }, _canShow: boolean): RowClick => ({ to: "open" });
