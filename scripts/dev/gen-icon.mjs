// Generates the LinkFYR source icon (1024x1024 PNG) with zero dependencies.
// Mark: two interlocking chain links on a deep navy rounded square — the
// product is about connecting paths; the mark is two paths joined.
// Then `tauri icon` derives every platform size from this file.
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const S = 1024;
const px = new Uint8Array(S * S * 4);

// palette
const BG_TOP = [17, 22, 34];
const BG_BOT = [9, 11, 17];
const LINK_A = [91, 140, 255]; // accent blue
const LINK_B = [143, 176, 255]; // light accent

function put(x, y, r, g, b, a) {
  if (x < 0 || y < 0 || x >= S || y >= S) return;
  const i = (y * S + x) * 4;
  const na = a / 255;
  px[i] = Math.round(px[i] * (1 - na) + r * na);
  px[i + 1] = Math.round(px[i + 1] * (1 - na) + g * na);
  px[i + 2] = Math.round(px[i + 2] * (1 - na) + b * na);
  px[i + 3] = Math.max(px[i + 3], a);
}

// rounded-rect signed distance
function sdRoundRect(x, y, cx, cy, hw, hh, r) {
  const dx = Math.abs(x - cx) - (hw - r);
  const dy = Math.abs(y - cy) - (hh - r);
  const ox = Math.max(dx, 0);
  const oy = Math.max(dy, 0);
  return Math.hypot(ox, oy) + Math.min(Math.max(dx, dy), 0) - r;
}

function stroke(alphaFn, x, y, d, halfW) {
  const cov = Math.max(0, 1 - (Math.abs(d) - halfW + 0.5));
  if (cov > 0) alphaFn(x, y, cov * 255);
}

const HW = 340; // background half-width
const BR = 230; // background radius
const LINK_W = 60; // link stroke half-width

for (let y = 0; y < S; y++) {
  for (let x = 0; x < S; x++) {
    // background: vertical gradient + rounded-rect alpha
    const t = y / S;
    const bg = [
      BG_TOP[0] + (BG_BOT[0] - BG_TOP[0]) * t,
      BG_TOP[1] + (BG_BOT[1] - BG_TOP[1]) * t,
      BG_TOP[2] + (BG_BOT[2] - BG_TOP[2]) * t,
    ];
    const dBg = sdRoundRect(x, y, S / 2, S / 2, HW, HW, BR);
    const aBg = Math.max(0, Math.min(1, 0.5 - dBg)) * 255;
    put(x, y, bg[0], bg[1], bg[2], aBg);

    // links: two rounded rects, overlapping horizontally, drawn twice each
    // to create the interlock (A over B on one edge, B over A on the other).
    const linkA = sdRoundRect(x, y, 400, 512, 150, 96, 96);
    const linkB = sdRoundRect(x, y, 624, 512, 150, 96, 96);

    const paintA = (px_, py_, cov) => {
      // break A where B passes through, except where we want the over-crossing
      const inB = Math.abs(linkB) < LINK_W;
      const rightHalf = x > 512;
      if (inB && !rightHalf) return; // B over A on the left crossing
      put(px_, py_, LINK_A[0], LINK_A[1], LINK_A[2], cov);
    };
    const paintB = (px_, py_, cov) => {
      const inA = Math.abs(linkA) < LINK_W;
      if (inA && x > 512) return; // A over B on the right crossing
      put(px_, py_, LINK_B[0], LINK_B[1], LINK_B[2], cov);
    };

    stroke(paintA, x, y, linkA, LINK_W);
    stroke(paintB, x, y, linkB, LINK_W);
  }
}

// ---- PNG encode (RGBA8, filter 0) ----
function crc32(buf) {
  let c;
  const table = crc32.table ??= (() => {
    const t = new Uint32Array(256);
    for (let n = 0; n < 256; n++) {
      c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      t[n] = c >>> 0;
    }
    return t;
  })();
  c = 0xffffffff;
  for (const byte of buf) c = table[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

const stride = S * 4;
const raw = Buffer.alloc((stride + 1) * S);
for (let y = 0; y < S; y++) {
  raw[y * (stride + 1)] = 0; // filter none
  Buffer.from(px.buffer, y * stride, stride).copy(raw, y * (stride + 1) + 1);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(S, 0);
ihdr.writeUInt32BE(S, 4);
ihdr[8] = 8; // depth
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "apps", "desktop", "src-tauri", "icons");
mkdirSync(out, { recursive: true });
writeFileSync(join(out, "icon.png"), png);
console.log(`wrote ${join(out, "icon.png")} (${png.length} bytes)`);
