// Generates placeholder app icons with no external dependencies.
//
// Replace these properly later with `cargo tauri icon path/to/logo.png`, which
// produces the full platform set from a single source image. These exist so the
// app builds today.

import { deflateSync } from "node:zlib";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const OUT = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");

// Dark slate ground with a burnt-orange chevron, roughly TF2's palette.
const BG = [0x1c, 0x1f, 0x26, 0xff];
const FG = [0xcf, 0x6a, 0x32, 0xff];

/** Solid background with a chevron, as a flat RGBA buffer. */
function render(size) {
  const px = Buffer.alloc(size * size * 4);
  const bar = Math.max(2, Math.round(size * 0.14));
  const mid = size / 2;

  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      // Distance from a V shape opening upward.
      const arm = Math.abs(x - mid);
      const onChevron =
        Math.abs(y - (size * 0.32 + arm * 0.9)) < bar / 2 && x > size * 0.12 && x < size * 0.88;
      const color = onChevron ? FG : BG;
      px.set(color, (y * size + x) * 4);
    }
  }
  return px;
}

// ---- PNG ------------------------------------------------------------------

const CRC_TABLE = (() => {
  const t = new Int32Array(256);
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[n] = c;
  }
  return t;
})();

function crc32(buf) {
  let c = -1;
  for (const b of buf) c = CRC_TABLE[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function png(size) {
  const px = render(size);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // RGBA
  // Each scanline is prefixed with filter type 0 (none).
  const raw = Buffer.alloc(size * (size * 4 + 1));
  for (let y = 0; y < size; y++) {
    raw[y * (size * 4 + 1)] = 0;
    px.copy(raw, y * (size * 4 + 1) + 1, y * size * 4, (y + 1) * size * 4);
  }
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

// ---- ICO ------------------------------------------------------------------

/** BMP-encoded rather than PNG-encoded: widest compatibility with the Windows
 *  resource compiler that embeds this into the executable. */
function icoImage(size) {
  const px = render(size);
  const header = Buffer.alloc(40);
  header.writeUInt32LE(40, 0);
  header.writeInt32LE(size, 4);
  header.writeInt32LE(size * 2, 8); // doubled: colour data + AND mask
  header.writeUInt16LE(1, 12); // planes
  header.writeUInt16LE(32, 14); // bpp
  header.writeUInt32LE(size * size * 4, 20);

  // BGRA, bottom-up.
  const body = Buffer.alloc(size * size * 4);
  for (let y = 0; y < size; y++) {
    const src = (size - 1 - y) * size * 4;
    for (let x = 0; x < size; x++) {
      const s = src + x * 4;
      const d = (y * size + x) * 4;
      body[d] = px[s + 2];
      body[d + 1] = px[s + 1];
      body[d + 2] = px[s];
      body[d + 3] = px[s + 3];
    }
  }
  // Fully opaque AND mask, padded to 4-byte rows.
  const maskRow = Math.ceil(size / 32) * 4;
  return Buffer.concat([header, body, Buffer.alloc(maskRow * size)]);
}

function ico(sizes) {
  const images = sizes.map(icoImage);
  const dir = Buffer.alloc(6 + 16 * sizes.length);
  dir.writeUInt16LE(0, 0);
  dir.writeUInt16LE(1, 2); // type: icon
  dir.writeUInt16LE(sizes.length, 4);

  let offset = dir.length;
  sizes.forEach((size, i) => {
    const at = 6 + i * 16;
    dir[at] = size >= 256 ? 0 : size; // 0 means 256
    dir[at + 1] = size >= 256 ? 0 : size;
    dir.writeUInt16LE(1, at + 4); // planes
    dir.writeUInt16LE(32, at + 6); // bpp
    dir.writeUInt32LE(images[i].length, at + 8);
    dir.writeUInt32LE(offset, at + 12);
    offset += images[i].length;
  });
  return Buffer.concat([dir, ...images]);
}

// ---- write ----------------------------------------------------------------

mkdirSync(OUT, { recursive: true });
for (const [name, size] of [
  ["32x32.png", 32],
  ["128x128.png", 128],
  ["128x128@2x.png", 256],
  ["icon.png", 512],
]) {
  writeFileSync(join(OUT, name), png(size));
  console.log(`wrote ${name} (${size}px)`);
}
writeFileSync(join(OUT, "icon.ico"), ico([16, 32, 48, 256]));
console.log("wrote icon.ico (16/32/48/256)");
