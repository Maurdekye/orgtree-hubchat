import React from "react";
import { createRoot } from "react-dom/client";
import { Avatar, PlatformCtx } from "../src/components/ui";
import { Directory } from "../src/components/Directory";
import { api } from "../src/api";
import { mockApi } from "../src/lib/mock";
import { startStore } from "../src/lib/store";
import { avatarFallback, avatarPng } from "../src/lib/avatarFallback";
import { taskIconPng } from "../src/lib/shortcutIcons";
import { Icon, TASK_ICONS } from "../src/lib/icons";
import "../src/styles/tokens.css";
import "../src/styles/base.css";

const root = createRoot(document.getElementById("root")!);
Object.assign(window, { taskIconPng, TASK_ICONS, mountTaskIcon: (view: keyof typeof TASK_ICONS) => {
  root.render(<button className="icon-btn"><Icon name={TASK_ICONS[view]} /></button>);
} });
const opened: string[] = [];
Object.assign(window, { opened, mountDirectory: async (platform: "desktop" | "android") => {
  Object.assign(api, mockApi);
  await startStore();
  root.render(<PlatformCtx.Provider value={platform}><Directory onOpen={address => opened.push(address)} onClose={() => {}} onBack={() => {}} /></PlatformCtx.Provider>);
} });
Object.assign(window, { avatarFallback, avatarPng, mountAvatar: (kind: "person" | "org", name: string) => {
  root.render(<Avatar kind={kind} name={name} size={44} />);
} });
