import type { MenuRow, MenuSnapshot } from "../src/traymenu-view";
export function fixture(dark = false, submenuLeft = true): MenuSnapshot {
  const row = (id: string, label: string, enabled = true, checked: boolean | null = null, children: MenuRow[] = []): MenuRow =>
    ({ id, label, enabled, checked, children, separator: false });
  const sep = (): MenuRow => ({ ...row("separator", "", false), separator: true });
  return { generation: 1, revision: 1, dark, submenuLeft, maxWidth: 1920, maxHeight: 1040, rows: [
    row("open", "Open Hubchat"), sep(), row("version", "Version 1.2.0-alpha.3", false),
    row("update-status", "Hubchat is up to date", false), row("update-auto", "Check for updates automatically", true, true),
    row("update-check", "Check for updates"), sep(), row("autostart", "Start with Windows", true, true),
    row("exit-on-close", "Exit on close", true, false),
    row("notifications", "Notifications", true, null, [row("notify-on", "Show notifications", true, true), sep(),
      row("notify-text", "Show message text", true, true), row("notify-sound", "Play a sound", true, false)]),
    row("crash-reports", "Crash reports…"), sep(), row("restart", "Restart Hubchat"), row("quit", "Quit Hubchat"),
  ] };
}
