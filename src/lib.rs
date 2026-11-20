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

#[macro_use]
mod kernel;

use std::borrow::Cow;

use crate::kernel::{
    count_digits, grouped32, grouped64, Fwd, Kernel, Raw, GROUP2, GROUP3, INDIAN, MIN_GROUP1,
    MIN_GROUP2, MIN_GROUP3,
};

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
    /// 正の値の前に付ける文字列。NumberFormat は使わない（無視する）
    pub positive_prefix: &'a str,
    /// 正の値の後に付ける文字列。NumberFormat は使わない（無視する）
    pub positive_suffix: &'a str,
    /// 負の値の前に付ける文字列＝負符号（通常 "-"）
    pub negative_prefix: &'a str,
    /// 負の値の後に付ける文字列。NumberFormat は使わない（無視する）
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

/// 速い経路で扱う小数桁数の集合（ビット k が立っていれば k 桁）。features で決まる
const NUMBER_FAST_FRAC: u16 = if cfg!(feature = "number-frac") {
    0b1_1111_1111
} else {
    0
}; // 0〜8
const CURRENCY_FAST_FRAC: u16 = (cfg!(feature = "currency-frac-0") as u16)
    | (cfg!(feature = "currency-frac-2") as u16) << 2
    | (cfg!(feature = "currency-frac-3") as u16) << 3;

// 速い経路は小数 0〜8 桁だけ（dispatch_d! の腕）
const _: () = assert!(NUMBER_FAST_FRAC < 1 << 9 && CURRENCY_FAST_FRAC < 1 << 9);

/// 数値の書式（検証・前計算済み）
#[derive(Debug, Clone)]
pub struct NumberFormat<'a> {
    decimal: Cow<'a, str>,
    group: Cow<'a, str>,
    /// 負符号（負の値の前にだけ付ける）
    minus_sign: Cow<'a, str>,
    primary: u8,
    secondary: u8,
    min_grouping: u8,
    frac: u8,
    /// どの書き出し方を使うか。構築時に一度だけ決める
    kernel: Kernel,
    /// 1 回の書き出しで増えうる最大バイト数（unsafe な書き込みの上限）
    max_len: usize,
}

impl<'a> NumberFormat<'a> {
    /// 値を検証して書式を作る
    pub fn new(p: NumberParams<'a>) -> Result<Self, FormatError> {
        if p.frac_digits > 19 {
            return Err(FormatError::FracDigitsTooLarge);
        }
        if p.min_grouping == 0 {
            return Err(FormatError::MinGroupingZero);
        }
        if p.primary_grouping > 19 || p.secondary_grouping > 19 {
            return Err(FormatError::GroupingTooLarge);
        }
        let primary = p.primary_grouping;
        // 第2グループ幅 0 は「第1グループと同じ」とみなす
        let secondary = if p.secondary_grouping == 0 {
            primary
        } else {
            p.secondary_grouping
        };
        // 速い経路の条件: 小数桁数が集合に含まれ、区切り方の特殊化があり、
        // （区切るなら）minimumGroupingDigits が集合に含まれること
        let frac_ok = p.frac_digits <= 8 && NUMBER_FAST_FRAC & (1 << p.frac_digits) != 0;
        let mg_ok = match p.min_grouping {
            1 => MIN_GROUP1,
            2 => MIN_GROUP2,
            3 => MIN_GROUP3,
            _ => false,
        };
        let kernel = match (frac_ok, primary, secondary) {
            (true, 3, 3) if GROUP3 && mg_ok => Kernel::G33,
            (true, 3, 2) if INDIAN && mg_ok => Kernel::G32,
            (true, 2, 2) if GROUP2 && mg_ok => Kernel::G22,
            _ => Kernel::Generic,
        };
        // 数字は最大 20 桁（u64）＋0 の補い 1 桁。区切りは数字の数より少ない
        let max_len = p.negative_prefix.len() + 21 + 20 * p.group.len() + p.decimal.len();
        Ok(NumberFormat {
            decimal: Cow::Borrowed(p.decimal),
            group: Cow::Borrowed(p.group),
            minus_sign: Cow::Borrowed(p.negative_prefix),
            primary,
            secondary,
            min_grouping: p.min_grouping,
            frac: p.frac_digits,
            kernel,
            max_len,
        })
    }

    /// 小数桁数（値のスケール）
    pub fn frac_digits(&self) -> u8 {
        self.frac
    }

    /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
    #[inline]
    pub fn write_i32(&self, out: &mut String, value: i32) {
        self.write_u32(out, value < 0, value.unsigned_abs())
    }

    /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
    #[inline]
    pub fn write_i64(&self, out: &mut String, value: i64) {
        self.write_u64(out, value < 0, value.unsigned_abs())
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

    #[inline]
    fn write_u64(&self, out: &mut String, neg: bool, m: u64) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m);
        }
        let (g, dec, mg) = (
            self.group.as_bytes(),
            self.decimal.as_bytes(),
            self.min_grouping as u32,
        );
        // SAFETY: 書き込むのは ASCII 数字と、&str（区切り・小数点・負符号）を丸ごとなので
        // UTF-8 は保たれる。書き込み量は構築時に計算した max_len 以内で、その分を先に reserve している
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Raw { p: start };
            if neg {
                w.s(self.minus_sign.as_bytes());
            }
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped64::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped64::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped64::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            let n = w.p.offset_from(start) as usize;
            debug_assert!(n <= self.max_len);
            v.set_len(v.len() + n);
        }
    }

    #[inline]
    fn write_u32(&self, out: &mut String, neg: bool, m: u32) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m as u64);
        }
        let (g, dec, mg) = (
            self.group.as_bytes(),
            self.decimal.as_bytes(),
            self.min_grouping as u32,
        );
        // SAFETY: write_u64 と同じ（u32 は最大 10 桁なので max_len に収まる）
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Raw { p: start };
            if neg {
                w.s(self.minus_sign.as_bytes());
            }
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped32::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped32::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(NUMBER_FAST_FRAC, self.frac, |D| grouped32::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            let n = w.p.offset_from(start) as usize;
            debug_assert!(n <= self.max_len);
            v.set_len(v.len() + n);
        }
    }

    /// 速い経路に当てはまらない書式用（FBL 方式）。
    /// 比較だけで桁数を求め、branchlut と同じ分岐で上の桁から生成しながら、
    /// 1 文字ごとに残り桁数を数えて区切りと小数点を挟む。任意の幅・minimumGroupingDigits・小数 0〜19 桁に対応
    fn write_generic(&self, out: &mut String, neg: bool, m: u64) {
        let d = self.frac as u32;
        let n = count_digits(m);
        let total = n.max(d + 1); // 0.05 のような値は先頭に 0 を補う
        let int_len = total - d;
        let (p, sc) = (self.primary as u32, self.secondary as u32);
        let grouping = p > 0 && int_len >= p + self.min_grouping as u32;
        // 先頭グループの桁数（区切らないときは整数部全体）
        let first = if grouping {
            let h = int_len - p;
            match h % sc {
                0 => sc,
                r => r,
            }
        } else {
            int_len
        };
        // SAFETY: 速い経路と同じ。書き込むのは ASCII 数字と &str を丸ごとで、量は max_len 以内
        // （数字は最大 20 桁、区切りはそれより少ない）。その分を先に reserve している
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Fwd {
                w: Raw { p: start },
                int_left: int_len,
                grp_left: first,
                primary: p,
                secondary: sc,
                has_frac: d > 0,
                group: self.group.as_bytes(),
                decimal: self.decimal.as_bytes(),
            };
            if neg {
                w.w.s(self.minus_sign.as_bytes());
            }
            for _ in n..total {
                w.digit(b'0');
            }
            w.u64(m);
            let written = w.w.p.offset_from(start) as usize;
            debug_assert!(written <= self.max_len);
            v.set_len(v.len() + written);
        }
    }
}

/// 通貨の書式（検証・前計算済み）
#[derive(Debug, Clone)]
pub struct CurrencyFormat<'a> {
    decimal: Cow<'a, str>,
    group: Cow<'a, str>,
    /// 非負の値の [前, 後]
    nonneg_affix: [Cow<'a, str>; 2],
    /// 負の値の [前, 後]
    neg_affix: [Cow<'a, str>; 2],
    primary: u8,
    secondary: u8,
    min_grouping: u8,
    frac: u8,
    /// どの書き出し方を使うか。構築時に一度だけ決める
    kernel: Kernel,
    /// 1 回の書き出しで増えうる最大バイト数（unsafe な書き込みの上限）
    max_len: usize,
}

impl<'a> CurrencyFormat<'a> {
    /// 値を検証して書式を作る
    pub fn new(p: CurrencyParams<'a>) -> Result<Self, FormatError> {
        if p.frac_digits > 19 {
            return Err(FormatError::FracDigitsTooLarge);
        }
        if p.min_grouping == 0 {
            return Err(FormatError::MinGroupingZero);
        }
        if p.primary_grouping > 19 || p.secondary_grouping > 19 {
            return Err(FormatError::GroupingTooLarge);
        }
        let primary = p.primary_grouping;
        // 第2グループ幅 0 は「第1グループと同じ」とみなす
        let secondary = if p.secondary_grouping == 0 {
            primary
        } else {
            p.secondary_grouping
        };
        // 速い経路の条件: 小数桁数が集合に含まれ、区切り方の特殊化があり、
        // （区切るなら）minimumGroupingDigits が集合に含まれること
        let frac_ok = p.frac_digits <= 8 && CURRENCY_FAST_FRAC & (1 << p.frac_digits) != 0;
        let mg_ok = match p.min_grouping {
            1 => MIN_GROUP1,
            2 => MIN_GROUP2,
            3 => MIN_GROUP3,
            _ => false,
        };
        let kernel = match (frac_ok, primary, secondary) {
            (true, 3, 3) if GROUP3 && mg_ok => Kernel::G33,
            (true, 3, 2) if INDIAN && mg_ok => Kernel::G32,
            (true, 2, 2) if GROUP2 && mg_ok => Kernel::G22,
            _ => Kernel::Generic,
        };
        let aff = (p.positive_prefix.len() + p.positive_suffix.len())
            .max(p.negative_prefix.len() + p.negative_suffix.len());
        // 数字は最大 20 桁（u64）＋0 の補い 1 桁。区切りは数字の数より少ない
        let max_len = aff + 21 + 20 * p.group.len() + p.decimal.len();
        Ok(CurrencyFormat {
            decimal: Cow::Borrowed(p.decimal),
            group: Cow::Borrowed(p.group),
            nonneg_affix: [
                Cow::Borrowed(p.positive_prefix),
                Cow::Borrowed(p.positive_suffix),
            ],
            neg_affix: [
                Cow::Borrowed(p.negative_prefix),
                Cow::Borrowed(p.negative_suffix),
            ],
            primary,
            secondary,
            min_grouping: p.min_grouping,
            frac: p.frac_digits,
            kernel,
            max_len,
        })
    }

    /// 小数桁数（値のスケール）
    pub fn frac_digits(&self) -> u8 {
        self.frac
    }

    /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
    #[inline]
    pub fn write_i32(&self, out: &mut String, value: i32) {
        self.write_u32(out, value < 0, value.unsigned_abs())
    }

    /// `value`（小数桁数をスケールとする整数）を `out` の末尾に追記する
    #[inline]
    pub fn write_i64(&self, out: &mut String, value: i64) {
        self.write_u64(out, value < 0, value.unsigned_abs())
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

    #[inline]
    fn write_u64(&self, out: &mut String, neg: bool, m: u64) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m);
        }
        let affix = if neg {
            &self.neg_affix
        } else {
            &self.nonneg_affix
        };
        let (g, dec, mg) = (
            self.group.as_bytes(),
            self.decimal.as_bytes(),
            self.min_grouping as u32,
        );
        // SAFETY: 書き込むのは ASCII 数字と、&str（区切り・小数点・前後の文字列）を丸ごとなので
        // UTF-8 は保たれる。書き込み量は構築時に計算した max_len 以内で、その分を先に reserve している
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Raw { p: start };
            w.s(affix[0].as_bytes());
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped64::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped64::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped64::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            w.s(affix[1].as_bytes());
            let n = w.p.offset_from(start) as usize;
            debug_assert!(n <= self.max_len);
            v.set_len(v.len() + n);
        }
    }

    #[inline]
    fn write_u32(&self, out: &mut String, neg: bool, m: u32) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m as u64);
        }
        let affix = if neg {
            &self.neg_affix
        } else {
            &self.nonneg_affix
        };
        let (g, dec, mg) = (
            self.group.as_bytes(),
            self.decimal.as_bytes(),
            self.min_grouping as u32,
        );
        // SAFETY: write_u64 と同じ（u32 は最大 10 桁なので max_len に収まる）
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Raw { p: start };
            w.s(affix[0].as_bytes());
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped32::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped32::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(CURRENCY_FAST_FRAC, self.frac, |D| grouped32::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            w.s(affix[1].as_bytes());
            let n = w.p.offset_from(start) as usize;
            debug_assert!(n <= self.max_len);
            v.set_len(v.len() + n);
        }
    }

    /// 速い経路に当てはまらない書式用（FBL 方式）。
    /// 比較だけで桁数を求め、branchlut と同じ分岐で上の桁から生成しながら、
    /// 1 文字ごとに残り桁数を数えて区切りと小数点を挟む。任意の幅・minimumGroupingDigits・小数 0〜19 桁に対応
    fn write_generic(&self, out: &mut String, neg: bool, m: u64) {
        let d = self.frac as u32;
        let n = count_digits(m);
        let total = n.max(d + 1); // 0.05 のような値は先頭に 0 を補う
        let int_len = total - d;
        let (p, sc) = (self.primary as u32, self.secondary as u32);
        let grouping = p > 0 && int_len >= p + self.min_grouping as u32;
        // 先頭グループの桁数（区切らないときは整数部全体）
        let first = if grouping {
            let h = int_len - p;
            match h % sc {
                0 => sc,
                r => r,
            }
        } else {
            int_len
        };
        let affix = if neg {
            &self.neg_affix
        } else {
            &self.nonneg_affix
        };
        // SAFETY: 速い経路と同じ。書き込むのは ASCII 数字と &str を丸ごとで、量は max_len 以内
        // （数字は最大 20 桁、区切りはそれより少ない）。その分を先に reserve している
        unsafe {
            let v = out.as_mut_vec();
            v.reserve(self.max_len);
            let start = v.as_mut_ptr().add(v.len());
            let mut w = Fwd {
                w: Raw { p: start },
                int_left: int_len,
                grp_left: first,
                primary: p,
                secondary: sc,
                has_frac: d > 0,
                group: self.group.as_bytes(),
                decimal: self.decimal.as_bytes(),
            };
            w.w.s(affix[0].as_bytes());
            for _ in n..total {
                w.digit(b'0');
            }
            w.u64(m);
            w.w.s(affix[1].as_bytes());
            let written = w.w.p.offset_from(start) as usize;
            debug_assert!(written <= self.max_len);
            v.set_len(v.len() + written);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::Kernel;

    /// 有効な features から期待される書き出し方
    #[allow(clippy::match_like_matches_macro)]
    fn expected(p: u8, s: u8, mg: u8, d: u8, mask: u16) -> Kernel {
        let s = if s == 0 { p } else { s };
        let frac_ok = d <= 8 && mask & (1 << d) != 0;
        let mg_ok = match mg {
            1 => cfg!(feature = "min-group-1"),
            2 => cfg!(feature = "min-group-2"),
            3 => cfg!(feature = "min-group-3"),
            _ => false,
        };
        match (p, s) {
            _ if !frac_ok => Kernel::Generic,
            (3, 3) if cfg!(feature = "group-3") && mg_ok => Kernel::G33,
            (3, 2) if cfg!(feature = "group-indian") && mg_ok => Kernel::G32,
            (2, 2) if cfg!(feature = "group-2") && mg_ok => Kernel::G22,
            _ => Kernel::Generic,
        }
    }

    #[test]
    fn fast_path_selection() {
        for (p, s) in [
            (3, 3),
            (3, 2),
            (3, 0),
            (2, 2),
            (2, 0),
            (0, 0),
            (0, 3),
            (2, 3),
            (4, 4),
            (1, 1),
            (3, 4),
        ] {
            for mg in 1..=4 {
                for d in 0..=19 {
                    let cur = CurrencyFormat::new(CurrencyParams {
                        decimal: ".",
                        group: ",",
                        primary_grouping: p,
                        secondary_grouping: s,
                        min_grouping: mg,
                        frac_digits: d,
                        positive_prefix: "",
                        positive_suffix: "",
                        negative_prefix: "-",
                        negative_suffix: "",
                    })
                    .unwrap();
                    assert_eq!(
                        cur.kernel,
                        expected(p, s, mg, d, CURRENCY_FAST_FRAC),
                        "currency {p},{s} mg={mg} d={d}"
                    );
                    let num = NumberFormat::new(NumberParams {
                        decimal: ".",
                        group: ",",
                        primary_grouping: p,
                        secondary_grouping: s,
                        min_grouping: mg,
                        frac_digits: d,
                        positive_prefix: "",
                        positive_suffix: "",
                        negative_prefix: "-",
                        negative_suffix: "",
                    })
                    .unwrap();
                    assert_eq!(
                        num.kernel,
                        expected(p, s, mg, d, NUMBER_FAST_FRAC),
                        "number {p},{s} mg={mg} d={d}"
                    );
                }
            }
        }
    }
}
