// Prints updater-<KEY>.json: {"<KEY>": {"asset": ASSET, "signature": <contents of SIG>}}
const fs = require("fs");
const { KEY, ASSET, SIG } = process.env;
if (!KEY || !ASSET || !SIG) throw new Error("KEY, ASSET and SIG are required");
const signature = fs.readFileSync(SIG, "utf8").trim();
if (!signature) throw new Error(`empty signature in ${SIG}`);
process.stdout.write(JSON.stringify({ [KEY]: { asset: ASSET, signature } }, null, 2) + "\n");
