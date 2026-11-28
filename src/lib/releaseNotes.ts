// Every release's notes, bundled for the changelog in Settings (changelog.ts). A release adds its
// docs/release-notes-<v>.md here in the same commit (docs/ci-release.md step 2; the release workflow
// refuses a tag whose notes are missing here, and tools/changelog.test.ts checks the list).

import v0_1_3 from "../../docs/release-notes-0.1.3.md?raw";
import v1_0_0 from "../../docs/release-notes-1.0.0.md?raw";
import v1_0_1 from "../../docs/release-notes-1.0.1.md?raw";
import v1_0_2 from "../../docs/release-notes-1.0.2.md?raw";
import v1_0_3 from "../../docs/release-notes-1.0.3.md?raw";
import v1_0_4 from "../../docs/release-notes-1.0.4.md?raw";
import v1_0_5 from "../../docs/release-notes-1.0.5.md?raw";
import v1_1_0 from "../../docs/release-notes-1.1.0.md?raw";
import v1_2_0 from "../../docs/release-notes-1.2.0.md?raw";
import v1_2_1 from "../../docs/release-notes-1.2.1.md?raw";
import v1_2_2 from "../../docs/release-notes-1.2.2.md?raw";
import v1_2_3 from "../../docs/release-notes-1.2.3.md?raw";
import v1_2_4 from "../../docs/release-notes-1.2.4.md?raw";
import v1_2_5 from "../../docs/release-notes-1.2.5.md?raw";
import v1_2_6 from "../../docs/release-notes-1.2.6.md?raw";
import v1_2_7 from "../../docs/release-notes-1.2.7.md?raw";
import v1_3_0 from "../../docs/release-notes-1.3.0.md?raw";
import v1_3_1 from "../../docs/release-notes-1.3.1.md?raw";
import v1_3_2 from "../../docs/release-notes-1.3.2.md?raw";
import v1_3_3 from "../../docs/release-notes-1.3.3.md?raw";
import v1_3_4 from "../../docs/release-notes-1.3.4.md?raw";
import v1_3_5 from "../../docs/release-notes-1.3.5.md?raw";
import v1_3_6 from "../../docs/release-notes-1.3.6.md?raw";
import v1_3_7 from "../../docs/release-notes-1.3.7.md?raw";
import v1_3_8 from "../../docs/release-notes-1.3.8.md?raw";
import v1_3_9 from "../../docs/release-notes-1.3.9.md?raw";
import v1_3_10 from "../../docs/release-notes-1.3.10.md?raw";
import v1_3_11 from "../../docs/release-notes-1.3.11.md?raw";
import v1_3_12 from "../../docs/release-notes-1.3.12.md?raw";
import v1_3_13 from "../../docs/release-notes-1.3.13.md?raw";
import v1_4_0 from "../../docs/release-notes-1.4.0.md?raw";
import v1_4_1 from "../../docs/release-notes-1.4.1.md?raw";
import v1_4_2 from "../../docs/release-notes-1.4.2.md?raw";

export const RELEASE_NOTES: { file: string; text: string }[] = [
  { file: "release-notes-0.1.3.md", text: v0_1_3 },
  { file: "release-notes-1.0.0.md", text: v1_0_0 },
  { file: "release-notes-1.0.1.md", text: v1_0_1 },
  { file: "release-notes-1.0.2.md", text: v1_0_2 },
  { file: "release-notes-1.0.3.md", text: v1_0_3 },
  { file: "release-notes-1.0.4.md", text: v1_0_4 },
  { file: "release-notes-1.0.5.md", text: v1_0_5 },
  { file: "release-notes-1.1.0.md", text: v1_1_0 },
  { file: "release-notes-1.2.0.md", text: v1_2_0 },
  { file: "release-notes-1.2.1.md", text: v1_2_1 },
  { file: "release-notes-1.2.2.md", text: v1_2_2 },
  { file: "release-notes-1.2.3.md", text: v1_2_3 },
  { file: "release-notes-1.2.4.md", text: v1_2_4 },
  { file: "release-notes-1.2.5.md", text: v1_2_5 },
  { file: "release-notes-1.2.6.md", text: v1_2_6 },
  { file: "release-notes-1.2.7.md", text: v1_2_7 },
  { file: "release-notes-1.3.0.md", text: v1_3_0 },
  { file: "release-notes-1.3.1.md", text: v1_3_1 },
  { file: "release-notes-1.3.2.md", text: v1_3_2 },
  { file: "release-notes-1.3.3.md", text: v1_3_3 },
  { file: "release-notes-1.3.4.md", text: v1_3_4 },
  { file: "release-notes-1.3.5.md", text: v1_3_5 },
  { file: "release-notes-1.3.6.md", text: v1_3_6 },
  { file: "release-notes-1.3.7.md", text: v1_3_7 },
  { file: "release-notes-1.3.8.md", text: v1_3_8 },
  { file: "release-notes-1.3.9.md", text: v1_3_9 },
  { file: "release-notes-1.3.10.md", text: v1_3_10 },
  { file: "release-notes-1.3.11.md", text: v1_3_11 },
  { file: "release-notes-1.3.12.md", text: v1_3_12 },
  { file: "release-notes-1.3.13.md", text: v1_3_13 },
  { file: "release-notes-1.4.0.md", text: v1_4_0 },
  { file: "release-notes-1.4.1.md", text: v1_4_1 },
  { file: "release-notes-1.4.2.md", text: v1_4_2 },
];
