// Downloads the Whisper model that ships inside the app (docs/ARCHITECTURE.md
// "Building"). The app itself never touches the network; this is a dev/build
// step only. Verifies the SHA-256 so a bad download can't end up bundled.
import { createHash } from "node:crypto";
import { createReadStream, existsSync, mkdirSync, renameSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";

const MODEL = {
  url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-base.en.bin",
  path: "src-tauri/resources/models/ggml-base.en.bin",
  sha256: "a03779c86df3323075f5e796cb2ce5029f00ec8869eee3fdfb897afe36c6d002",
};

function sha256(path) {
  return new Promise((resolve, reject) => {
    const hash = createHash("sha256");
    createReadStream(path).on("data", (d) => hash.update(d)).on("end", () => resolve(hash.digest("hex"))).on("error", reject);
  });
}

if (existsSync(MODEL.path) && (await sha256(MODEL.path)) === MODEL.sha256) {
  console.log(`${MODEL.path} is present and verified.`);
  process.exit(0);
}

console.log(`Downloading ${MODEL.url} …`);
const res = await fetch(MODEL.url);
if (!res.ok) throw new Error(`download failed: HTTP ${res.status}`);
mkdirSync(dirname(MODEL.path), { recursive: true });
const tmp = `${MODEL.path}.part`;
writeFileSync(tmp, Buffer.from(await res.arrayBuffer()));
const got = await sha256(tmp);
if (got !== MODEL.sha256) throw new Error(`checksum mismatch: expected ${MODEL.sha256}, got ${got}`);
renameSync(tmp, MODEL.path);
console.log(`Saved and verified ${MODEL.path}.`);
