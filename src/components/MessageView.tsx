// One message: question tag, reply quotes, file cards, markdown body with the
// time and ticks, a question card's options, the failed line, and
// (desktop) the hover actions.
import { memo, useState, type KeyboardEvent, type MouseEvent } from "react";
import { api, type Attachment, type Message } from "../api";
import { Icon, iconHTML } from "../lib/icons";
import { isLong, num, time } from "../lib/format";
import { esc, md } from "../lib/md";
import { copyText, errText, openAttachment, openFile, openLink } from "../lib/native";
import { attView, msgTime, preview, tickInfo, type Kind } from "../lib/peers";
import { foldAt, replyIds } from "../lib/replies";
import { cardOf, type AnswerNote, type CardState } from "../lib/questions";
import { openImage, previewable } from "../lib/images";
import { openMenu, selectionIn, type MenuEntry } from "../lib/ctxmenu";
import { useTransfer } from "../lib/store";
import { toast } from "../lib/toast";
import { AttImage } from "./AttImage";
import { pressKeys, usePlatform } from "./ui";

function metaHTML(m: Message): string {
  let h = '<span class="meta"><span class="mt">' + time(msgTime(m)) + "</span>";
  if (m.outgoing) { const t = tickInfo(m); h += '<span class="tick ' + t.cls + '" title="' + esc(t.label) + '" role="img" aria-label="' + esc(t.label) + '">' + iconHTML(t.ic) + "</span>"; }
  return h + "</span>";
}

/** An attachment: an image shows as itself (falling back to its card),
 *  anything else as a file card. */
export function AttCard({ a, m }: { a: Attachment; m: Message }) {
  const card = <FileCard a={a} m={m} />;
  return previewable(a) ? <AttImage a={a} m={m} fallback={card} /> : card;
}

function FileCard({ a, m }: { a: Attachment; m: Message }) {
  const platform = usePlatform();
  const prog = useTransfer(a.local_id);
  const v = attView(a, m, prog && (a.state === "uploading" || a.state === "downloading") ? prog : undefined);
  const remove = m.outgoing && m.state === "failed" && !a.download;
  const download = () => api.download(m.id, a.local_id).catch((e) => toast("Download failed: " + errText(e)));
  const click = () => {
    if (v.download || v.retry === "download") void download();
    else if (v.retry === "send") api.retry(m.id).catch((e) => toast(errText(e)));
    else if (v.open && a.local_path) openAttachment(m.id, a.local_id, false).catch((e) => toast(errText(e)));
    else if (m.outgoing && a.source && v.st === "local" && !a.source.startsWith("content://")) openFile(a.source).catch(() => {});
  };
  return (
    <div className={"att-file " + v.st} data-att={a.local_id} onClick={click} title={a.name} role="button" tabIndex={0} onKeyDown={pressKeys(click)}>
      <span className="att-ic"><Icon name={v.ic} /></span>
      <span className="att-t">
        <span className="att-n">{a.name}</span>
        <span className="att-s">{v.sub}{v.retry ? " · Retry" : ""}</span>
        {remove ? <button className="link" onClick={(e) => { e.stopPropagation(); api.removeAttachment(m.id,a.local_id).catch(err=>toast(errText(err))); }}>Remove file</button> : null}
        {v.bar != null ? <span className="att-bar"><i style={{ width: v.bar + "%" }} /></span> : null}
        {v.open && a.local_path && platform === "desktop" ? <button className="link" style={{ fontSize: 12, alignSelf: "flex-start", marginTop: 2 }} onClick={(e) => { e.stopPropagation(); openAttachment(m.id, a.local_id, true).catch((err) => toast(errText(err))); }}>Show in folder</button> : null}
      </span>
      {v.download ? <span className="att-dl"><Icon name="download" /></span> : null}
      {v.cancel ? <button className="att-x" title={m.outgoing && !a.download ? "Cancel upload" : "Cancel download"} onClick={(e) => { e.stopPropagation(); api.cancelTransfer(a.local_id).catch(e => toast(errText(e))); }}><Icon name="close" /></button> : null}
    </div>
  );
}

/** A picture's or a file's own actions, for the right-click menu: what its
 *  card and the picture viewer offer. */
function attMenu(m: Message, a: Attachment): MenuEntry[] {
  const v = attView(a, m);
  const out: MenuEntry[] = [];
  const download = () => void api.download(m.id, a.local_id).catch((e) => toast("Download failed: " + errText(e)));
  if (previewable(a)) out.push({ label: "View", icon: "image", run: () => openImage(m, a) });
  if (v.download) out.push({ label: "Download", icon: "download", run: download });
  if (v.retry === "download") out.push({ label: "Retry download", icon: "refresh", run: download });
  if (v.retry === "send") out.push({ label: "Retry upload", icon: "refresh", run: () => void api.retry(m.id).catch((e) => toast(errText(e))) });
  if (v.open && a.local_path) {
    out.push({ label: "Open", icon: "open_in_new", run: () => void openAttachment(m.id, a.local_id, false).catch((e) => toast(errText(e))) });
    out.push({ label: "Show in folder", icon: "folder", run: () => void openAttachment(m.id, a.local_id, true).catch((e) => toast(errText(e))) });
  } else if (m.outgoing && a.source && v.st === "local" && !a.source.startsWith("content://")) {
    const src = a.source;
    out.push({ label: "Open", icon: "open_in_new", run: () => void openFile(src).catch(() => {}) });
  }
  if (v.cancel) out.push({ label: m.outgoing && !a.download ? "Cancel upload" : "Cancel download", icon: "close", run: () => void api.cancelTransfer(a.local_id).catch(e => toast(errText(e))) });
  if (m.outgoing && m.state === "failed" && !a.download) out.push({label:"Remove file",icon:"close",run:()=>void api.removeAttachment(m.id,a.local_id).catch(e=>toast(errText(e)))});
  return out;
}

export interface MsgHandlers {
  onReply: (m: Message) => void;
  /** Add the message to the selection, or take it out. */
  onSelect?: (m: Message) => void;
  /** Pick (or unpick) an option of a question card. */
  onPick?: (m: Message, label: string) => void;
  onInfo: (m: Message) => void;
  onDelete: (m: Message) => void;
  onJump: (id: string) => void;
  onOpenAddr: (address: string) => void;
}

interface Props extends MsgHandlers {
  m: Message;
  first: boolean;
  peerName: string;
  peerKind: Kind;
  quoted: Message | null | undefined;
  /** A multi-reply: one quote per message it answers (null: not on this device). */
  quotes?: (Message | null | undefined)[];
  /** A question card: where it stands. */
  card?: CardState;
  /** A question card: the options picked for the next message. */
  picks?: string[];
  /** A reply to question cards: what it did to each. */
  notes?: AnswerNote[];
  hover: boolean;
  selected?: boolean;
  /** Desktop: the keyboard's highlight (Shift+Tab walks it, R replies). */
  highlighted?: boolean;
}

export const MessageView = memo(function MessageView({ m, first, peerName, peerKind, quoted, quotes, card, picks, notes, hover, selected, highlighted, onReply, onSelect, onPick, onInfo, onDelete, onJump, onOpenAddr }: Props) {
  const platform = usePlatform();
  const [open, setOpen] = useState(false);
  const [allQuotes, setAllQuotes] = useState(false);
  const long = !!m.body && isLong(m.body);

  const delegate = (e: MouseEvent<HTMLDivElement>) => {
    const t = e.target as HTMLElement;
    const copy = t.closest("[data-act=copy-code]");
    if (copy) { const pre = copy.closest(".code")?.querySelector("pre"); if (pre) void copyText(pre.innerText, "Code copied"); e.stopPropagation(); return; }
    const link = t.closest("a.md-a") as HTMLElement | null;
    if (link) { e.preventDefault(); void openLink(link.dataset.href || ""); return; }
    const addr = t.closest("a.addr") as HTMLElement | null;
    if (addr) { e.preventDefault(); onOpenAddr(addr.dataset.slug || ""); return; }
    // desktop: Ctrl+click adds the message to the selection (multi-reply)
    if ((e.ctrlKey || e.metaKey) && platform === "desktop" && !t.closest("button, [data-att], .quote") && onSelect) { e.preventDefault(); onSelect(m); }
  };

  // a link or address (no href: it is opened by us) by keyboard
  const keyOpen = (e: KeyboardEvent<HTMLDivElement>) => {
    if (e.key !== "Enter" && e.key !== " ") return;
    const t = e.target as HTMLElement;
    if (t.matches("a.md-a")) { e.preventDefault(); void openLink(t.dataset.href || ""); }
    else if (t.matches("a.addr")) { e.preventDefault(); onOpenAddr(t.dataset.slug || ""); }
  };

  // what this reply did to a question card it answers
  const noteEl = (id: string) => {
    const n = notes?.find((x) => x.id === id);
    const t = !n ? "" : !n.counted ? "Not counted: answered earlier" : n.picked ? "Answer: " + n.picked.join(", ") : n.skipped ? "Skipped" : "";
    return t ? <span className={"q-ans" + (n?.counted ? "" : " stale")}>{t}</span> : null;
  };
  const quoteEl = (q: Message | null | undefined, id: string) => {
    if (q === null) return <div className="quote missing" key={id}>Original message is not on this device{noteEl(id)}</div>;
    if (!q) return null;
    const kc = q.outgoing ? "k-me" : peerKind === "person" ? "" : "k-" + peerKind;
    return (
      <div className={"quote " + kc} key={id} onClick={(e) => { e.stopPropagation(); onJump(q.id); }} title="Show the original message">
        <span className="q-who">{q.outgoing ? "You" : peerName}</span>
        <span className="q-txt" dir="auto">{preview(q).slice(0, 140)}</span>
        {noteEl(id)}
      </div>
    );
  };
  let quote = null;
  const ids = replyIds(m);
  if (ids.length > 1 && quotes) {
    // several: stacked, a long list folded (up to three show whole)
    const n = foldAt(quotes.length), folded = !allQuotes && n < quotes.length;
    quote = (
      <div className="quotes">
        {(folded ? quotes.slice(0, n) : quotes).map((q, i) => quoteEl(q, ids[i]))}
        {folded ? <button className="quote-more" onClick={(e) => { e.stopPropagation(); setAllQuotes(true); }}>and {quotes.length - n} more</button> : null}
      </div>
    );
  } else if (m.reply_to) quote = quoteEl(quoted, m.reply_to);

  // a question card: its header says how to answer, or how it was answered
  // (the hint starts a reply, user 2026-10-11 13:24Z); its options wait in
  // the composer until the next message goes (13:03Z)
  const qc = cardOf(m);
  const settled = card && card.st !== "open" ? card : null;
  const canPick = !!qc && !settled && !m.outgoing && !!onPick;
  let qhead = null, qopts = null, qfoot = null;
  if (qc) {
    qhead = (
      <div className="qhead">
        <div className="kchip kc-q">Question</div>
        {canPick ? <button className="q-hint" onClick={(e) => { e.stopPropagation(); onReply(m); }}>{qc.options.length ? "Reply to answer something else" : "Reply to answer"}</button>
          : settled ? <button className={"q-state " + settled.st} onClick={(e) => { e.stopPropagation(); onJump(settled.by.id); }} title="Show the answer">{settled.st === "skipped" ? "Skipped" : "Answered"} · {time(msgTime(settled.by))}</button> : null}
      </div>
    );
    if (qc.options.length) {
      const chosen = settled?.st === "answered" ? settled.picked ?? [] : canPick ? picks ?? [] : [];
      qopts = (
        <div className={"q-opts" + (qc.multiple ? " multi" : "")} role={qc.multiple ? "group" : "radiogroup"} aria-label={qc.multiple ? "Pick one or more" : "Pick one"}>
          {qc.options.map((o) => {
            const on = chosen.includes(o);
            return (
              <button key={o} className={"q-opt" + (on ? " on" : "")} role={qc.multiple ? "checkbox" : "radio"} aria-checked={on} disabled={!canPick} onClick={(e) => { e.stopPropagation(); onPick?.(m, o); }}>
                <span className="q-mark">{on ? <Icon name="check" /> : null}</span><span className="q-l" dir="auto">{o}</span>
              </button>
            );
          })}
        </div>
      );
      if (canPick) qfoot = <div className="q-note">{picks?.length ? "Send your picked answer" : qc.multiple ? "Pick one or more" : "Pick one"}</div>;
      else if (settled?.st === "answered" && !settled.picked) qfoot = <div className="q-note">Answered in a reply</div>;
    }
  }
  const metaLast = !!qopts;
  const meta = metaHTML(m);
  let body = null;
  if (m.body && long) {
    body = (
      <>
        <div className={"mbody" + (open ? "" : " clamp")} dir="auto" dangerouslySetInnerHTML={{ __html: md(m.body) }} />
        <button className="more" onClick={(e) => { e.stopPropagation(); setOpen(!open); }}>{open ? "Show less" : "Show more · " + num(m.body.length) + " characters"}</button>
        {metaLast ? null : <div className="meta-line" dangerouslySetInnerHTML={{ __html: meta }} />}
      </>
    );
  } else if (m.body && metaLast) {
    body = <div className="mtext" dir="auto" dangerouslySetInnerHTML={{ __html: md(m.body) }} />;
  } else if (m.body) {
    let h = md(m.body);
    h = /<\/p>$/.test(h) ? h.slice(0, -4) + meta + "</p>" : h + '<div class="meta-line">' + meta + "</div>";
    body = <div className="mtext" dir="auto" dangerouslySetInnerHTML={{ __html: h }} />;
  } else if (!metaLast) {
    body = <div className="meta-line" dangerouslySetInnerHTML={{ __html: meta }} />;
  }

  const failed = m.state === "failed";
  // desktop right-click (user 2026-10-09 05:54Z): what was clicked first (a
  // link, an address, a picture or a file), then the message's own actions,
  // as its hover bar and failed line offer them
  const menu = (e: MouseEvent<HTMLDivElement>) => {
    if (platform !== "desktop") return;
    e.preventDefault();
    const t = e.target as HTMLElement;
    const items: MenuEntry[] = [];
    const link = t.closest("a.md-a") as HTMLElement | null;
    const addr = t.closest("a.addr") as HTMLElement | null;
    const att = t.closest("[data-att]") as HTMLElement | null;
    if (link) {
      const href = link.dataset.href || "";
      items.push({ label: "Open link", icon: "open_in_new", run: () => void openLink(href) }, { label: "Copy link", icon: "link", run: () => void copyText(href, "Link copied") }, "sep");
    } else if (addr) {
      const slug = addr.dataset.slug || "";
      items.push({ label: "Open chat", icon: "forum", run: () => onOpenAddr(slug) }, { label: "Copy address", icon: "copy", run: () => void copyText("@net:" + slug, "Address copied") }, "sep");
    } else if (att) {
      const a = m.attachments.find((x) => x.local_id === att.dataset.att);
      if (a) items.push(...attMenu(m, a), "sep");
    }
    const sel = selectionIn(e.currentTarget);
    items.push({ label: "Reply", icon: "reply", run: () => onReply(m) });
    if (onSelect) items.push({ label: selected ? "Unselect" : "Select", icon: "check_circle", run: () => onSelect(m) });
    if (sel) items.push({ label: "Copy selection", icon: "copy", run: () => void copyText(sel, "Copied") });
    else if (m.body) items.push({ label: "Copy text", icon: "copy", run: () => void copyText(m.body, "Copied") });
    items.push({ label: "Message info", icon: "info", run: () => onInfo(m) });
    if (failed) items.push("sep", { label: "Retry", icon: "refresh", run: () => void api.retry(m.id).catch((er) => toast(errText(er))) }, { label: "Delete", icon: "delete", bad: true, run: () => onDelete(m) });
    openMenu(e.clientX, e.clientY, items);
  };
  return (
    <div className={"msg " + (m.outgoing ? "out" : "in") + (first ? " first" : "") + (selected ? " sel" : "") + (highlighted ? " hl" : "")} id={"m-" + m.id} data-id={m.id}>
      {platform === "android" ? <span className="swipe-ic"><Icon name="reply" /></span> : null}
      <div className="row">
        <div className={"bubble" + (m.attachments.length && !m.body ? " only-att" : "") + (m.reply_to ? " has-quote" : "") + (qc ? " qcard" : "")} onClick={delegate} onKeyDown={keyOpen} onContextMenu={menu}>
          <span className="sr">{(m.outgoing ? "You" : peerName) + (qc ? ", Question" : "") + ": "}</span>
          {qhead}
          {quote}
          {m.attachments.length ? <div className="atts">{m.attachments.map((a) => <AttCard key={a.local_id} a={a} m={m} />)}</div> : null}
          {body}
          {qopts}
          {qfoot}
          {metaLast ? <div className="meta-line" dangerouslySetInnerHTML={{ __html: meta }} /> : null}
        </div>
        {/* beside the bubble, in the empty space, never over its text (user 23:38Z) */}
        {hover ? (
          <div className="msg-actions">
            <button className="icon-btn" title="Reply" onClick={() => onReply(m)}><Icon name="reply" /></button>
            <button className="icon-btn" title="Copy text" onClick={() => copyText(m.body, "Copied")}><Icon name="copy" /></button>
            <button className="icon-btn" title="Message info" onClick={() => onInfo(m)}><Icon name="info" /></button>
          </div>
        ) : null}
      </div>
      {failed ? (
        <div className="failed-line">
          <Icon name="error" />
          <span>Not sent{m.error ? ": " + m.error : ""}.</span>
          <button className="link" onClick={() => api.retry(m.id).catch((e) => toast(errText(e)))}>Retry</button>
          <button className="link" onClick={() => onDelete(m)}>Delete</button>
        </div>
      ) : null}
    </div>
  );
});
