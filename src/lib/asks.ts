// A chat that has an incoming question card waiting for an answer (the taskbar dot's rule, docket
// hubchat-notification-pip-stuck-on-with-no-unread).

/** What the marker on a chat says to a screen reader and on hover. */
export const needsAnswerLabel = (n: number): string => n + (n === 1 ? " question needs" : " questions need") + " your answer";

export type RowClick = { to: "open" } | { to: "question"; id: string };

/** What a click on a chat row does: open the chat, or open it and show its oldest pending card. */
export const rowClick = (c: { needs_answer: number; first_question?: string | null }, canShow: boolean): RowClick =>
  canShow && c.needs_answer > 0 && c.first_question ? { to: "question", id: c.first_question } : { to: "open" };
