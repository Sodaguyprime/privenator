const NUMERALS: [number, string][] = [
  [1000, "M"], [900, "CM"], [500, "D"], [400, "CD"],
  [100, "C"], [90, "XC"], [50, "L"], [40, "XL"],
  [10, "X"], [9, "IX"], [5, "V"], [4, "IV"], [1, "I"],
];

/** 1 → "I", 14 → "XIV". Used for volume and entry numbering (docs/DESIGN.md). */
export function toRoman(n: number): string {
  if (!Number.isInteger(n) || n < 1 || n > 3999) throw new RangeError(`toRoman: ${n}`);
  let out = "";
  for (const [value, glyph] of NUMERALS) {
    while (n >= value) {
      out += glyph;
      n -= value;
    }
  }
  return out;
}
