# pprint-currency
Pretty printer for currency and number on Rust

固定小数点の整数（i32 / i64）を、ロケールの書式（区切り・小数点・前後の文字列）で文字列にする。

- 値は「小数桁数をスケールとする整数」で渡す（小数 2 桁なら 1234.56 → 123456）。丸めは行わない。
- 数字は ASCII のみ。
- CLDR のデータは含まない。書式の値は自分で渡すか、`tools/gen.py` で CLDR から作る。
- 速い経路: 3 桁区切りかインド式（3, 2）、minimumGroupingDigits 1 か 2、小数桁数が Currency 0・2 / Number 0〜8。
  それ以外（他の幅、通貨の小数 3 桁など）も正しく出力する（FBL 方式の遅い経路）。

```rust
use pprint_currency::{CurrencyFormat, CurrencyParams, NumberFormat, NumberParams};

// 作るときに検証と前計算を済ませるので、作ったものは使い回す
let jpy = CurrencyFormat::new(CurrencyParams {
    decimal: ".", group: ",", grouping: 3.into(), min_grouping: 1, frac_digits: 0,
    positive_prefix: "￥", positive_suffix: "", negative_prefix: "-￥", negative_suffix: "",
})?;
let mut out = String::new();
jpy.write_i64(&mut out, -1234567);   // out に "-￥1,234,567" を追記
jpy.write_i32(&mut out, 1000);       // out に "￥1,000" を追記

let units = NumberFormat::new(NumberParams {
    decimal: ".", group: ",", grouping: 3.into(), min_grouping: 1, frac_digits: 6,
    positive_prefix: "", positive_suffix: "", negative_prefix: "-", negative_suffix: "",
})?;
assert_eq!(units.format_i64(1234567890), "1,234.567890");
```

## features（特殊化の選択）

どの特殊化を入れるかを Cargo の features で選べる。どれを切っても出力は同じで、
切った分は遅い経路（FBL 方式）で処理される。速い経路を通るのは、
「区切り方」「minimumGroupingDigits」「小数桁数」の特殊化がすべて有効なときだけ。

| 種類 | feature | 既定 |
|---|---|---|
| 区切り方 | `group-3`（3 桁）/ `group-indian`（3, 2）/ `group-2`（2 桁） | 3 桁・インド式 |
| minimumGroupingDigits | `min-group-1` / `min-group-2` / `min-group-3` | 1・2 |
| 通貨の小数桁数 | `currency-frac-0` / `currency-frac-2` / `currency-frac-3` | 0・2 |
| 数値の小数桁数 | `number-frac`（0〜8 桁をまとめて） | オン |

features は足し算なので、一部だけ使うときは `default-features = false` にして必要なものを並べる。

```toml
# 例: 日本円とドルだけ速く（3 桁区切り・minimumGroupingDigits 1・小数 0 と 2）、数値は特殊化なし
pprint-currency = { version = "...", default-features = false,
                   features = ["group-3", "min-group-1", "currency-frac-0", "currency-frac-2"] }
```

## CLDR から値を作る

`tools/gen.py` は cldr-json（cldr-numbers-full/main/<locale>/{numbers,currencies}.json と
cldr-core/supplemental/{currencyData,numberingSystems}.json を置いたフォルダ）から値を作る。

```sh
# 必要な通貨だけ Rust の定数として出力（自分のクレートに貼る）
python3 tools/gen.py --rust <root> ja:JPY,USD,EUR en:USD > src/formats.rs
#   pub const JA_JPY: CurrencyParams<'static> = CurrencyParams { ... };
#   pub const JA_NUMBER: NumberParams<'static> = NumberParams { ..., frac_digits: 0 };  // 小数桁数は用途に合わせて上書き

# 全通貨を JSON で出力（テストや実行時の読み込み用）
python3 tools/gen.py <root> ja en de > params.json
```

ASCII 以外の数字のロケールや、未対応のパターン構文に出会うと例外で止まる。

## テスト

```sh
python3 tools/gen.py <root> ja en en-IN de de-CH de-AT fr es sv he pt-PT > tests/cldr_params.json
node tools/golden.js <root> ja en en-IN de de-CH de-AT fr es sv he pt-PT > tests/golden.json
cargo test --release && cargo test
```

`tests/cldr_params.json` と `tests/golden.json`（Node の Intl.NumberFormat の出力、約 23MB）は生成物なのでリポジトリに含めていない。
無いときは Intl との照合テストは何もせずに終わる（参照実装との比較テストは常に動く）。
Node が持つ CLDR のバージョンと cldr-json のバージョンを揃えること
（Node 22.23.4 は CLDR 48.0 で、cldr-json 48.2.0 と一致した。48.0.0 とは RSD の小数桁数が違った）。

速度の目安: `cargo run --release --example bench`

## Acknowledgements

The integer-to-string conversion is based on the `branchlut` algorithm from
[miloyip/itoa-benchmark](https://github.com/miloyip/itoa-benchmark) by Milo Yip.
It uses comparison-based branching to determine the number of digits and a two-digit lookup table
to write digits from the most significant end. The slow path (`count_digits` and `Fwd` in `src/kernel.rs`)
is a port of its branching structure, and the fast path builds on the same idea, writing whole digit groups
(2 / 2+1 / 2+2 digits) so that no per-digit separator check is needed.

itoa-benchmark is distributed under the MIT License:

```
Copyright (C) 2014 Milo Yip

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.
```
