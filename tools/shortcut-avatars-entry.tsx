import React from "react";
import { createRoot } from "react-dom/client";
import { Avatar } from "../src/components/ui";
import { avatarFallback, avatarPng } from "../src/lib/avatarFallback";
import "../src/styles/tokens.css";
import "../src/styles/base.css";

const root = createRoot(document.getElementById("root")!);
Object.assign(window, { avatarFallback, avatarPng, mountAvatar: (kind: "person" | "org", name: string) => {
  root.render(<Avatar kind={kind} name={name} size={44} />);
} });
