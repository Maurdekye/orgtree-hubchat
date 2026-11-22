import { createElement } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { api } from "../src/api";
import * as store from "../src/lib/store";
import { SettingsModal, SettingsSection } from "../src/components/Settings";
import { Onboarding } from "../src/components/Onboarding";
import { SetupFlow } from "../src/components/SetupFlow";
import { MessageView } from "../src/components/MessageView";
import { OsCtx, PlatformCtx, WebCtx, defaultDeviceName, detectOs, useDeviceWord } from "../src/components/ui";
import { orefContext, parseOref, tokenChip } from "../src/lib/orgtreeRefs";

function DeviceWordProbe() {
  const word = useDeviceWord();
  return createElement("span", { id: "device-word" }, word);
}

export {
  createElement,
  act,
  createRoot,
  api,
  store,
  SettingsModal,
  SettingsSection,
  Onboarding,
  SetupFlow,
  MessageView,
  OsCtx,
  PlatformCtx,
  WebCtx,
  DeviceWordProbe,
  defaultDeviceName,
  detectOs,
  orefContext,
  parseOref,
  tokenChip,
};
