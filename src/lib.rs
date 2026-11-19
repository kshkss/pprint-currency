//! 固定小数点の整数を、ロケールの書式（区切り・小数点・記号）で文字列にする。
//!
//! - 書式の情報（区切り・小数点・小数桁数・前後の文字列）は [`NumberParams`] / [`CurrencyParams`] で渡し、
//!   `new` で [`NumberFormat`] / [`CurrencyFormat`] を作る。作るときに検証と前計算を済ませるので、作ったものは使い回す。
//! - CLDR のデータは含まない。CLDR から値を作るなら tools/gen.py を使う。
//! - 速い経路: 3 桁区切りかインド式（3, 2）、minimumGroupingDigits 1 か 2、小数は Currency 0・2 桁 / Number 0〜8 桁。
//!   それ以外も正しく出力する（1 文字ごとに区切りを判定する遅い経路）。
//!   どの特殊化を入れるかは Cargo の features で選べる（Cargo.toml を参照）。出力は features によらず同じ。
//! - 値は「小数桁数をスケールとする整数」で渡す（例: 小数 2 桁なら 1234.56 → 123456）。
//!   丸めは行わない。
//! - 数字は ASCII（0-9）のみ。
//!
//! ```
//! use pprint_currency::{CurrencyFormat, CurrencyParams, NumberFormat, NumberParams};
//!
//! let jpy = CurrencyFormat::new(CurrencyParams {
//!     decimal: ".", group: ",", primary_grouping: 3, secondary_grouping: 3, min_grouping: 1, frac_digits: 0,
//!     positive_prefix: "￥", positive_suffix: "", negative_prefix: "-￥", negative_suffix: "",
//! }).unwrap();
//! let mut s = String::new();
//! jpy.write_i64(&mut s, -1234567);
//! assert_eq!(s, "-￥1,234,567");
//!
//! let de = NumberFormat::new(NumberParams {
//!     decimal: ",", group: ".", primary_grouping: 3, secondary_grouping: 3, min_grouping: 1, frac_digits: 2,
//!     positive_prefix: "", positive_suffix: "", negative_prefix: "-", negative_suffix: "",
//! }).unwrap();
//! assert_eq!(de.format_i32(123456), "1.234,56");
//! ```

mod kernel;

use crate::kernel::Layout;

/// 書式の値が不正なときのエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatError {
    /// 小数桁数が 19 を超えている
    FracDigitsTooLarge,
    /// minimumGroupingDigits が 0
    MinGroupingZero,
    /// グループ幅が 19 を超えている
    GroupingTooLarge,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            FormatError::FracDigitsTooLarge => "小数桁数は 19 以下にしてください",
            FormatError::MinGroupingZero => "min_grouping は 1 以上にしてください",
            FormatError::GroupingTooLarge => "グループ幅は 19 以下にしてください",
        })
    }
}
impl std::error::Error for FormatError {}

/// 数値の書式を作るための値
#[derive(Debug, Clone, Copy)]
pub struct NumberParams<'a> {
    /// 小数点（例: "." / ","）
    pub decimal: &'a str,
    /// 桁区切り（例: "," / "." / "\u{202f}"）
    pub group: &'a str,
    /// 第1グループの幅（右端のグループ。0 なら区切らない）
    pub primary_grouping: u8,
    /// 第2グループ以降の幅（0 なら primary_grouping と同じ。インド式なら 2）
    pub secondary_grouping: u8,
    /// 整数部がこの桁数＋primary_grouping 以上のときだけ区切る（CLDR の minimumGroupingDigits。通常 1）
    pub min_grouping: u8,
    /// 小数桁数（値のスケール）
    pub frac_digits: u8,
    /// 正の値の前に付ける文字列（通常 ""）
    pub positive_prefix: &'a str,
    /// 正の値の後に付ける文字列（通常 ""）
    pub positive_suffix: &'a str,
    /// 負の値の前に付ける文字列（通常 "-"）
    pub negative_prefix: &'a str,
    /// 負の値の後に付ける文字列（通常 ""）
    pub negative_suffix: &'a str,
}

/// 通貨の書式を作るための値。前後の文字列には通貨記号と空白を含める（例: 負の前 "-￥"、"-1.234,56 €" なら負の後 " €"）
#[derive(Debug, Clone, Copy)]
pub struct CurrencyParams<'a> {
    /// 小数点
    pub decimal: &'a str,
    /// 桁区切り
    pub group: &'a str,
    /// 第1グループの幅（0 なら区切らない）
    pub primary_grouping: u8,
    /// 第2グループ以降の幅（0 なら primary_grouping と同じ）
    pub secondary_grouping: u8,
    /// CLDR の minimumGroupingDigits（通常 1）
    pub min_grouping: u8,
    /// 小数桁数（その通貨の値のスケール）
    pub frac_digits: u8,
    /// 正の値の前（例: "￥", "$", "CHF "）
    pub positive_prefix: &'a str,
    /// 正の値の後（例: "", " €"）
    pub positive_suffix: &'a str,
    /// 負の値の前（例: "-￥", "CHF-"）
    pub negative_prefix: &'a str,
    /// 負の値の後
    pub negative_suffix: &'a str,
}

/// 数値の書式（検証・前計算済み）
#[derive(Debug, Clone)]
pub struct NumberFormat<'a>(Layout<'a>);

/// 通貨の書式（検証・前計算済み）
#[derive(Debug, Clone)]
pub struct CurrencyFormat<'a>(Layout<'a>);

macro_rules! common_impl {
    ($ty:ident, $params:ident, $fast_frac:expr) => {
        impl<'a> $ty<'a> {
            /// 値を検証して書式を作る
            pub fn new(p: $params<'a>) -> Result<Self, FormatError> {
                Layout::new(
                    p.decimal,
                    p.group,
                    [
                        p.positive_prefix,
                        p.positive_suffix,
                        p.negative_prefix,
                        p.negative_suffix,
                    ],
                    p.primary_grouping,
                    p.secondary_grouping,
                    p.min_grouping,
                    p.frac_digits,
                    $fast_frac,
                )
                .map($ty)
            }

            /// 小数桁数（値のスケール）
            pub fn frac_digits(&self) -> u8 {
                self.0.frac_digits()
            }

            /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
            #[inline]
            pub fn write_i32(&self, out: &mut String, value: i32) {
                self.0
                    .write_u32::<{ $fast_frac }>(out, value < 0, value.unsigned_abs())
            }

            /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
            #[inline]
            pub fn write_i64(&self, out: &mut String, value: i64) {
                self.0
                    .write_u64::<{ $fast_frac }>(out, value < 0, value.unsigned_abs())
            }

            /// 新しい String に書き出す（便利用。速さが必要なら write_* でバッファを使い回す）
            pub fn format_i32(&self, value: i32) -> String {
                let mut s = String::new();
                self.write_i32(&mut s, value);
                s
            }

            /// 新しい String に書き出す（便利用）
            pub fn format_i64(&self, value: i64) -> String {
                let mut s = String::new();
                self.write_i64(&mut s, value);
                s
            }
        }
    };
}
/// 速い経路で扱う小数桁数の集合（ビット k が立っていれば k 桁）。features で決まる
const NUMBER_FAST_FRAC: u16 = if cfg!(feature = "number-frac") {
    0b1_1111_1111
} else {
    0
}; // 0〜8
const CURRENCY_FAST_FRAC: u16 = (cfg!(feature = "currency-frac-0") as u16)
    | (cfg!(feature = "currency-frac-2") as u16) << 2
    | (cfg!(feature = "currency-frac-3") as u16) << 3;

common_impl!(NumberFormat, NumberParams, NUMBER_FAST_FRAC);
common_impl!(CurrencyFormat, CurrencyParams, CURRENCY_FAST_FRAC);
