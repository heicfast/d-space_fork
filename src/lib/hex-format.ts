/**
 * Hex-dump row formatting — the ONE implementation, shared by the hex
 * worker (browser/WebView2) and the inline fallback (environments without
 * Worker). 16-byte rows, a double space after the 8th byte, printable-ASCII
 * substitution with '·', fixed-width hex padding for column alignment.
 */
export interface HexRow {
  offset: number;
  hex: string;
  ascii: string;
}

const HEX_TABLE: string[] = [];
for (let i = 0; i < 256; i++) HEX_TABLE.push(i.toString(16).padStart(2, '0'));

const HEX_ROW_WIDTH = 16 * 3 + 1;

export function formatHexRows(bytes: Uint8Array, baseOffset: number): HexRow[] {
  const rows: HexRow[] = [];
  for (let i = 0; i < bytes.length; i += 16) {
    const slice = bytes.subarray(i, i + 16);
    let hex = '';
    let ascii = '';
    for (let j = 0; j < slice.length; j++) {
      const b = slice[j];
      hex += HEX_TABLE[b] + (j === 7 ? '  ' : ' ');
      ascii += b >= 32 && b < 127 ? String.fromCharCode(b) : '·';
    }
    while (hex.length < HEX_ROW_WIDTH) hex += ' ';
    rows.push({ offset: baseOffset + i, hex, ascii });
  }
  return rows;
}
