// Generates tests/resources/brotli/minimal/brotli-minimal.pdf
// A whole small document in which every compressed thing is a Brotli stream:
// page content, an image with a PNG predictor, the object stream and the xref
// stream. Run: node pdf/tests/resources/brotli/minimal/make.mjs
import { brotliCompressSync, constants } from 'node:zlib';
import { writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const here = dirname(fileURLToPath(import.meta.url));
const brot = (buf, large = false) =>
  brotliCompressSync(buf, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: 11,
      [constants.BROTLI_PARAM_LGWIN]: large ? 24 : 22,
    },
  });

// ---- page 1: text, page 2: the image -------------------------------
const content1 = Buffer.from(
  'BT /F1 18 Tf 50 300 Td (Brotli-encoded page content) Tj ET\n' +
    '0.5 w 50 280 m 250 280 l S\n',
  'latin1',
);

const W = 64,
  H = 64;
const pixels = Buffer.alloc(W * H * 3);
for (let y = 0; y < H; y++)
  for (let x = 0; x < W; x++) {
    const i = (y * W + x) * 3;
    pixels[i] = (x * 4) & 0xff;
    pixels[i + 1] = (y * 4) & 0xff;
    pixels[i + 2] = 0x40;
  }
// PNG predictor, Up filter for every row after the first
const predicted = Buffer.alloc(H * (W * 3 + 1));
for (let y = 0; y < H; y++) {
  const o = y * (W * 3 + 1);
  predicted[o] = y === 0 ? 0 : 2;
  for (let k = 0; k < W * 3; k++) {
    const cur = pixels[y * W * 3 + k];
    const up = y === 0 ? 0 : pixels[(y - 1) * W * 3 + k];
    predicted[o + 1 + k] = (cur - up) & 0xff;
  }
}
const content2 = Buffer.from(`q ${W * 4} 0 0 ${H * 4} 20 20 cm /Im1 Do Q\n`, 'latin1');

// ---- objects -------------------------------------------------------
// 1 catalog, 2 pages, 3 page 1, 4 page 2, 5 font  -> object stream 9
// 6 content 1, 7 content 2, 8 image               -> plain
const dicts = {
  1: '<</Type/Catalog/Pages 2 0 R>>',
  2: '<</Type/Pages/Kids[3 0 R 4 0 R]/Count 2/MediaBox[0 0 300 400]/Resources<</Font<</F1 5 0 R>>>>>>',
  3: '<</Type/Page/Parent 2 0 R/Contents 6 0 R>>',
  4: '<</Type/Page/Parent 2 0 R/Contents 7 0 R/Resources<</XObject<</Im1 8 0 R>>>>>>',
  5: '<</Type/Font/Subtype/Type1/BaseFont/Helvetica/Encoding/WinAnsiEncoding>>',
};

const inStm = Object.keys(dicts).map(Number).sort((a, b) => a - b);
let header = '',
  body = '';
for (const n of inStm) {
  header += `${n} ${body.length} `;
  body += dicts[n] + '\n';
}
const objStm = Buffer.from(header + body, 'latin1');

const outs = [];
outs.push(['%PDF-2.0\n%\xe2\xe3\xcf\xd3\n', null]);

const push = (num, head, stream) => {
  let s = `${num} 0 obj\n${head}`;
  if (stream) {
    s += 'stream\n';
    const pre = Buffer.from(s, 'latin1');
    const post = Buffer.from('\nendstream\nendobj\n', 'latin1');
    outs.push([pre, null], [null, stream], [post, null]);
  } else {
    outs.push([s + '\nendobj\n', null]);
  }
};

const off = {};
const flush = () => {
  let len = 0;
  const parts = [];
  for (const [a, bbuf] of outs) {
    if (a === null) {
      parts.push(bbuf);
      len += bbuf.length;
    } else {
      parts.push(Buffer.from(a, 'latin1'));
      len += Buffer.from(a, 'latin1').length;
    }
  }
  return { buf: Buffer.concat(parts), len };
};

const mark = num => {
  off[num] = flush().len;
};

// placeholders so offsets are right: build sequentially
const contentB = brot(content1);
const content2B = brot(content2);
const imageB = brot(predicted);
const stmB = brot(objStm);
const xrefPlaceholder = () => {};

mark(6);
push(6, `<</Length ${contentB.length}/Filter/BrotliDecode>>`, contentB);
mark(7);
push(7, `<</Length ${content2B.length}/Filter/BrotliDecode>>`, content2B);
mark(8);
push(
  8,
  `<</Type/XObject/Subtype/Image/Width ${W}/Height ${H}/ColorSpace/DeviceRGB/BitsPerComponent 8/Length ${imageB.length}/Filter/BrotliDecode/DecodeParms<</Predictor 15/Colors 3/BitsPerComponent 8/Columns ${W}>>>>`,
  imageB,
);
mark(9);
push(9, `<</Type/ObjStm/N ${inStm.length}/First ${header.length}/Length ${stmB.length}/Filter/BrotliDecode>>`, stmB);

// xref stream, object 10: W [1 4 2]
const xrefStart = flush().len;
const entries = [];
const put = (t, b, c) => entries.push([t, b, c]);
put(0, 0, 65535); // object 0
for (let n = 1; n <= 10; n++) {
  if (dicts[n]) put(2, 9, inStm.indexOf(n));
  else if (off[n] !== undefined) put(1, off[n], 0);
  else if (n === 10) put(1, xrefStart, 0);
}
const xrow = Buffer.alloc(entries.length * 7);
let p = 0;
for (const [t, b, c] of entries) {
  xrow[p++] = t;
  xrow.writeUInt32BE(b, p);
  p += 4;
  xrow.writeUInt16BE(c, p);
  p += 2;
}
const xrefB = brot(xrow);
mark(10);
push(
  10,
  `<</Type/XRef/Size ${entries.length}/W[1 4 2]/Root 1 0 R/Length ${xrefB.length}/Filter/BrotliDecode>>`,
  xrefB,
);
const { buf, len } = flush();
const out = Buffer.concat([buf, Buffer.from(`\nstartxref\n${xrefStart}\n%%EOF\n`, 'latin1')]);
const dest = join(here, 'brotli-minimal.pdf');
writeFileSync(dest, out);
console.log('wrote', dest, out.length, 'bytes; xref at', xrefStart, 'total', len);
