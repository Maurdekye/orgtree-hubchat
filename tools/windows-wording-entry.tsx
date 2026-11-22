import { createElement } from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { api } from "../src/api";
import * as store from "../src/lib/store";
import { SettingsModal } from "../src/components/Settings";
import { Onboarding } from "../src/components/Onboarding";
import { PlatformCtx, WebCtx, defaultDeviceName, useDeviceWord } from "../src/components/ui";
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
  Onboarding,
  PlatformCtx,
  WebCtx,
  DeviceWordProbe,
  defaultDeviceName,
  orefContext,
  parseOref,
  tokenChip,
};
