// One message: kind chip, reply quote, file cards, markdown body with the
// time and ticks, the failed line, and (desktop) the hover actions.
import { memo, useState, type MouseEvent } from "react";
import { api, type Attachment, type Message } from "../api";
import { Icon, iconHTML } from "../lib/icons";
import { isLong, num, time } from "../lib/format";
import { esc, md } from "../lib/md";
import { copyText, errText, openAttachment, openFile, openLink } from "../lib/native";
import { attView, msgTime, preview, tickInfo, type Kind } from "../lib/peers";
import { previewable } from "../lib/images";
import { useTransfer } from "../lib/store";
import { toast } from "../lib/toast";
import { AttImage } from "./AttImage";
import { usePlatform } from "./ui";

const KCHIP: Record<string, [string, string]> = { question: ["Question", "kc-q"], request: ["Request", "kc-q"], decision: ["Decision", "kc-d"], status: ["Status", "kc-s"] };

function metaHTML(m: Message): string {
  let h = '<span class="meta"><span class="mt">' + time(msgTime(m)) + "</span>";
  if (m.outgoing) { const t = tickInfo(m); h += '<span class="tick ' + t.cls + '" title="' + esc(t.label) + '">' + iconHTML(t.ic) + "</span>"; }
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
  const download = () => api.download(m.id, a.local_id).catch((e) => toast("Download failed: " + errText(e)));
  const click = () => {
    if (v.download || v.retry === "download") void download();
    else if (v.retry === "send") api.retry(m.id).catch((e) => toast(errText(e)));
    else if (v.open && a.local_path) openAttachment(m.id, a.local_id, false).catch((e) => toast(errText(e)));
    else if (m.outgoing && a.source && v.st === "local" && !a.source.startsWith("content://")) openFile(a.source).catch(() => {});
  };
  return (
    <div className={"att-file " + v.st} onClick={click} title={a.name} role="button">
      <span className="att-ic"><Icon name={v.ic} /></span>
      <span className="att-t">
        <span className="att-n">{a.name}</span>
        <span className="att-s">{v.sub}{v.retry ? " · Retry" : ""}</span>
        {v.bar != null ? <span className="att-bar"><i style={{ width: v.bar + "%" }} /></span> : null}
        {v.open && a.local_path && platform === "desktop" ? <button className="link" style={{ fontSize: 12, alignSelf: "flex-start", marginTop: 2 }} onClick={(e) => { e.stopPropagation(); openAttachment(m.id, a.local_id, true).catch((err) => toast(errText(err))); }}>Show in folder</button> : null}
      </span>
      {v.download ? <span className="att-dl"><Icon name="download" /></span> : null}
      {v.cancel ? <button className="att-x" title={m.outgoing ? "Cancel upload" : "Cancel download"} onClick={(e) => { e.stopPropagation(); api.cancelTransfer(a.local_id).catch(() => {}); }}><Icon name="close" /></button> : null}
    </div>
  );
}

export interface MsgHandlers {
  onReply: (m: Message) => void;
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
  hover: boolean;
  selected?: boolean;
}

export const MessageView = memo(function MessageView({ m, first, peerName, peerKind, quoted, hover, selected, onReply, onInfo, onDelete, onJump, onOpenAddr }: Props) {
  const platform = usePlatform();
  const [open, setOpen] = useState(false);
  const long = !!m.body && isLong(m.body);

  const delegate = (e: MouseEvent<HTMLDivElement>) => {
    const t = e.target as HTMLElement;
    const copy = t.closest("[data-act=copy-code]");
    if (copy) { const pre = copy.closest(".code")?.querySelector("pre"); if (pre) void copyText(pre.innerText, "Code copied"); e.stopPropagation(); return; }
    const link = t.closest("a.md-a") as HTMLElement | null;
    if (link) { e.preventDefault(); void openLink(link.dataset.href || ""); return; }
    const addr = t.closest("a.addr") as HTMLElement | null;
    if (addr) { e.preventDefault(); onOpenAddr(addr.dataset.slug || ""); }
  };

  const k = m.kind ? KCHIP[m.kind] : undefined;
  let quote = null;
  if (m.reply_to) {
    if (quoted === null) quote = <div className="quote missing">Original message is not on this device</div>;
    else if (quoted) {
      const kc = quoted.outgoing ? "k-me" : peerKind === "person" ? "" : "k-" + peerKind;
      quote = (
        <div className={"quote " + kc} onClick={(e) => { e.stopPropagation(); onJump(quoted.id); }} title="Show the original message">
          <span className="q-who">{quoted.outgoing ? "You" : peerName}</span>
          <span className="q-txt">{preview(quoted).slice(0, 140)}</span>
        </div>
      );
    }
  }
  const meta = metaHTML(m);
  let body = null;
  if (m.body && long) {
    body = (
      <>
        <div className={"mbody" + (open ? "" : " clamp")} dangerouslySetInnerHTML={{ __html: md(m.body) }} />
        <button className="more" onClick={(e) => { e.stopPropagation(); setOpen(!open); }}>{open ? "Show less" : "Show more · " + num(m.body.length) + " characters"}</button>
        <div className="meta-line" dangerouslySetInnerHTML={{ __html: meta }} />
      </>
    );
  } else if (m.body) {
    let h = md(m.body);
    h = /<\/p>$/.test(h) ? h.slice(0, -4) + meta + "</p>" : h + '<div class="meta-line">' + meta + "</div>";
    body = <div className="mtext" dangerouslySetInnerHTML={{ __html: h }} />;
  } else {
    body = <div className="meta-line" dangerouslySetInnerHTML={{ __html: meta }} />;
  }

  const failed = m.state === "failed";
  return (
    <div className={"msg " + (m.outgoing ? "out" : "in") + (first ? " first" : "") + (selected ? " sel" : "")} id={"m-" + m.id} data-id={m.id}>
      {platform === "android" ? <span className="swipe-ic"><Icon name="reply" /></span> : null}
      <div className="row">
        <div className={"bubble" + (m.attachments.length && !m.body ? " only-att" : "") + (m.reply_to ? " has-quote" : "")} onClick={delegate}>
          {k ? <div className={"kchip " + k[1]}>{k[0]}</div> : null}
          {quote}
          {m.attachments.length ? <div className="atts">{m.attachments.map((a) => <AttCard key={a.local_id} a={a} m={m} />)}</div> : null}
          {body}
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
