// Node の Intl.NumberFormat で照合用の期待値を作る（CLDR のバージョンは Node 側に依存）
// 使い方: node tools/golden.js <cldr-json root> <locale>... > tests/golden.json
const fs = require("fs");
const [root, ...locs] = process.argv.slice(2);
const frac = JSON.parse(fs.readFileSync(`${root}/currencyData.json`)).supplemental.currencyData.fractions;
let x = 0x9E3779B9n;
const rnd = () => { x ^= (x << 13n) & 0xFFFFFFFFFFFFFFFFn; x ^= x >> 7n; x ^= (x << 17n) & 0xFFFFFFFFFFFFFFFFn; return x; };
const vals = [0n, 1n, -1n, 5n, -5n, 9223372036854775807n, -9223372036854775808n, 2147483647n, -2147483648n];
for (let k = 1n; k <= 18n; k++) { const p = 10n ** k; vals.push(p, p - 1n, -p, -(p - 1n), p + 1n); }
for (let i = 0; i < 40; i++) { const r = rnd(); const v = (r >> (r % 64n)) % 9223372036854775807n; vals.push(r & 1n ? -v : v); }
const dec = (m, d) => { const neg = m < 0n; let s = (neg ? -m : m).toString().padStart(d + 1, "0");
  if (d > 0) s = s.slice(0, s.length - d) + "." + s.slice(s.length - d); return (neg ? "-" : "") + s; };
const out = { currency: [], number: [] };
for (const l of locs) {
  const cur = JSON.parse(fs.readFileSync(`${root}/${l}/currencies.json`)).main[l].numbers.currencies;
  for (const code of Object.keys(cur)) {
    const d = +((frac[code] || frac.DEFAULT)._digits);
    let f; try { f = new Intl.NumberFormat(l, { style: "currency", currency: code }); } catch (e) { continue; }
    if (f.resolvedOptions().maximumFractionDigits !== d) throw new Error(`digits mismatch ${l} ${code}`);
    for (const m of vals) out.currency.push([l, code, m.toString(), f.format(dec(m, d))]);
  }
  for (const d of [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 12, 18]) {
    const f = new Intl.NumberFormat(l, { minimumFractionDigits: d, maximumFractionDigits: d });
    for (const m of vals) out.number.push([l, d, m.toString(), f.format(dec(m, d))]);
  }
}
process.stdout.write(JSON.stringify(out));
console.error(`currency ${out.currency.length} / number ${out.number.length} / Node ${process.version} CLDR ${process.versions.cldr}`);
