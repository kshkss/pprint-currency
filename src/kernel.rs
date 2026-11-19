//! 書き出しの本体。
//! 桁数の判定と数字の書き出しは miloyip/itoa-benchmark の branchlut（MIT、Copyright (C) 2014 Milo Yip）を
//! ベースにしている（README の Acknowledgements を参照）。
//! 固定小数点の整数を、グループ単位（2 / 2+1 / 2+2 … 桁）で
//! 区切りの判定なしに書き出す（FGR 方式）。数字は ASCII のみ。

use crate::FormatError;

pub(crate) const POW10: [u64; 20] = {
    let mut t = [1u64; 20];
    let mut i = 1;
    while i < 20 {
        t[i] = t[i - 1] * 10;
        i += 1;
    }
    t
};

const LUT: &[u8; 200] = b"0001020304050607080910111213141516171819\
2021222324252627282930313233343536373839\
4041424344454647484950515253545556575859\
6061626364656667686970717273747576777879\
8081828384858687888990919293949596979899";

/// 小数桁数（実行時の値）から、定数にした実装を選ぶ
macro_rules! dispatch_d {
    // $mask はコンパイル時定数（速い経路で扱う小数桁数の集合）。集合にない桁数の腕は最適化で消える
    ($mask:expr, $d:expr, |$D:ident| $call:expr) => {
        match $d {
            0 if $mask & (1 << 0) != 0 => { const $D: usize = 0; $call }
            1 if $mask & (1 << 1) != 0 => { const $D: usize = 1; $call }
            2 if $mask & (1 << 2) != 0 => { const $D: usize = 2; $call }
            3 if $mask & (1 << 3) != 0 => { const $D: usize = 3; $call }
            4 if $mask & (1 << 4) != 0 => { const $D: usize = 4; $call }
            5 if $mask & (1 << 5) != 0 => { const $D: usize = 5; $call }
            6 if $mask & (1 << 6) != 0 => { const $D: usize = 6; $call }
            7 if $mask & (1 << 7) != 0 => { const $D: usize = 7; $call }
            8 if $mask & (1 << 8) != 0 => { const $D: usize = 8; $call }
            // SAFETY: 構築時に、集合に含まれる小数桁数のときだけ速い経路を選んでいる（呼び出し側は unsafe 内）
            _ => { debug_assert!(false, "速い経路にない小数桁数"); core::hint::unreachable_unchecked() }
        }
    };
}

// どの特殊化を入れるか（Cargo の features で決まる）。false の特殊化はコードごと消える
/// 3 桁区切り（3, 3）
const GROUP3: bool = cfg!(feature = "group-3");
/// インド式（3, 2）
const INDIAN: bool = cfg!(feature = "group-indian");
/// 2 桁区切り（2, 2）
const GROUP2: bool = cfg!(feature = "group-2");
/// 速い経路で扱う minimumGroupingDigits
const MIN_GROUP1: bool = cfg!(feature = "min-group-1");
const MIN_GROUP2: bool = cfg!(feature = "min-group-2");
/// 3 を扱うと、3 桁区切りで区切らない整数部が 5 桁になりうる
const MIN_GROUP3: bool = cfg!(feature = "min-group-3");

/// どの書き出し方を使うか。構築時に一度だけ決める
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kernel {
    /// 速い経路: 3 桁区切り
    G33,
    /// 速い経路: インド式（第1グループ 3 桁、以降 2 桁）
    G32,
    /// 速い経路: 2 桁区切り
    G22,
    /// 上記以外（特殊化していない幅・minimumGroupingDigits・小数桁数）。
    /// FBL 方式（1 文字ごとに区切りを判定）で書く
    Generic,
}

/// 書式の情報（検証済み）。NumberFormat / CurrencyFormat の中身
#[derive(Clone, Debug)]
pub(crate) struct Layout<'a> {
    decimal: &'a str,
    group: &'a str,
    /// [正の前, 正の後, 負の前, 負の後]
    affix: [&'a str; 4],
    primary: u8,
    secondary: u8,
    min_grouping: u8,
    frac: u8,
    kernel: Kernel,
    /// 1 回の書き出しで増えうる最大バイト数（unsafe な書き込みの上限）
    max_len: usize,
}

impl<'a> Layout<'a> {
    /// `fast_frac_mask`: 速い経路で扱う小数桁数の集合（ビット k が立っていれば k 桁。0〜8 のみ）
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        decimal: &'a str,
        group: &'a str,
        affix: [&'a str; 4],
        primary: u8,
        secondary: u8,
        min_grouping: u8,
        frac: u8,
        fast_frac_mask: u16,
    ) -> Result<Self, FormatError> {
        debug_assert!(fast_frac_mask < 1 << 9);
        if frac > 19 {
            return Err(FormatError::FracDigitsTooLarge);
        }
        if min_grouping == 0 {
            return Err(FormatError::MinGroupingZero);
        }
        if primary > 19 || secondary > 19 {
            return Err(FormatError::GroupingTooLarge);
        }
        // 第2グループ幅 0 は「第1グループと同じ」とみなす
        let secondary = if secondary == 0 { primary } else { secondary };
        // 速い経路の条件: 小数桁数が集合に含まれ、区切り方の特殊化があり、
        // （区切るなら）minimumGroupingDigits が集合に含まれること
        let frac_ok = frac <= 8 && fast_frac_mask & (1 << frac) != 0;
        let mg_ok = match min_grouping {
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
        let aff = (affix[0].len() + affix[1].len()).max(affix[2].len() + affix[3].len());
        // 数字は最大 20 桁（u64）＋0 の補い 1 桁。区切りは数字の数より少ない
        let max_len = aff + 21 + 20 * group.len() + decimal.len();
        Ok(Layout {
            decimal,
            group,
            affix,
            primary,
            secondary,
            min_grouping,
            frac,
            kernel,
            max_len,
        })
    }

    pub(crate) fn frac_digits(&self) -> u8 {
        self.frac
    }

    #[inline]
    pub(crate) fn write_u64<const FAST_FRAC_MASK: u16>(&self, out: &mut String, neg: bool, m: u64) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m);
        }
        let k = neg as usize * 2;
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
            w.s(self.affix[k].as_bytes());
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped64::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped64::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped64::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            w.s(self.affix[k + 1].as_bytes());
            let n = w.p.offset_from(start) as usize;
            debug_assert!(n <= self.max_len);
            v.set_len(v.len() + n);
        }
    }

    #[inline]
    pub(crate) fn write_u32<const FAST_FRAC_MASK: u16>(&self, out: &mut String, neg: bool, m: u32) {
        if self.kernel == Kernel::Generic {
            return self.write_generic(out, neg, m as u64);
        }
        let k = neg as usize * 2;
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
            w.s(self.affix[k].as_bytes());
            match self.kernel {
                // 定数が false の腕は選ばれないので、最適化でコードごと消える
                Kernel::G33 if GROUP3 => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped32::<3, 3, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G32 if INDIAN => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped32::<3, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                Kernel::G22 if GROUP2 => {
                    dispatch_d!(FAST_FRAC_MASK, self.frac, |D| grouped32::<2, 2, D>(
                        &mut w, m, mg, g, dec
                    ))
                }
                // SAFETY: 構築時に、有効な特殊化のときだけ速い経路を選んでいる（Generic は先に返している）
                _ => {
                    debug_assert!(false, "無効な特殊化");
                    core::hint::unreachable_unchecked()
                }
            }
            w.s(self.affix[k + 1].as_bytes());
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
        let k = neg as usize * 2;
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
            w.w.s(self.affix[k].as_bytes());
            for _ in n..total {
                w.digit(b'0');
            }
            w.u64(m);
            w.w.s(self.affix[k + 1].as_bytes());
            let written = w.w.p.offset_from(start) as usize;
            debug_assert!(written <= self.max_len);
            v.set_len(v.len() + written);
        }
    }
}

/// 生ポインタへの書き込み（容量確認なし。呼び出し側で reserve 済み）
struct Raw {
    p: *mut u8,
}

impl Raw {
    #[inline(always)]
    unsafe fn b(&mut self, x: u8) {
        *self.p = x;
        self.p = self.p.add(1);
    }
    #[inline(always)]
    unsafe fn s(&mut self, s: &[u8]) {
        for &x in s {
            self.b(x)
        }
    }
    #[inline(always)]
    unsafe fn pair(&mut self, v: u32) {
        let i = v as usize * 2;
        self.b(LUT[i]);
        self.b(LUT[i + 1]);
    }
    /// ちょうど N 桁（v < 10^N、N <= 9）。2 桁ずつ、奇数なら残り 1 桁
    #[inline(always)]
    unsafe fn fixed<const N: usize>(&mut self, mut v: u32) {
        let end = self.p.add(N);
        let mut q = end;
        let mut i = 0;
        while i + 2 <= N {
            q = q.sub(2);
            let j = (v % 100) as usize * 2;
            *q = LUT[j];
            *q.add(1) = LUT[j + 1];
            v /= 100;
            i += 2;
        }
        if N % 2 == 1 {
            q = q.sub(1);
            *q = b'0' + v as u8;
        }
        self.p = end;
    }
    /// v < 10^4 を先頭の 0 なしで（branchlut と同じく比較で分岐）
    #[inline(always)]
    unsafe fn lead4(&mut self, v: u32) {
        if v >= 100 {
            if v >= 1000 {
                self.pair(v / 100);
                self.pair(v % 100);
            } else {
                self.b(b'0' + (v / 100) as u8);
                self.pair(v % 100);
            }
        } else if v >= 10 {
            self.pair(v)
        } else {
            self.b(b'0' + v as u8)
        }
    }
}

/// 比較だけで桁数を求める（branchlut と同じく値の範囲で分岐）
#[inline(always)]
fn count_digits(v: u64) -> u32 {
    if v < 100_000_000 {
        if v < 10_000 {
            if v < 100 {
                if v < 10 {
                    1
                } else {
                    2
                }
            } else if v < 1_000 {
                3
            } else {
                4
            }
        } else if v < 1_000_000 {
            if v < 100_000 {
                5
            } else {
                6
            }
        } else if v < 10_000_000 {
            7
        } else {
            8
        }
    } else if v < 10_000_000_000_000_000 {
        if v < 1_000_000_000_000 {
            if v < 10_000_000_000 {
                if v < 1_000_000_000 {
                    9
                } else {
                    10
                }
            } else if v < 100_000_000_000 {
                11
            } else {
                12
            }
        } else if v < 100_000_000_000_000 {
            if v < 10_000_000_000_000 {
                13
            } else {
                14
            }
        } else if v < 1_000_000_000_000_000 {
            15
        } else {
            16
        }
    } else if v < 100_000_000_000_000_000 {
        17
    } else if v < 1_000_000_000_000_000_000 {
        18
    } else if v < 10_000_000_000_000_000_000 {
        19
    } else {
        20
    }
}

/// 遅い経路の書き込み状態。数字を 1 文字書くたびに、区切り・小数点を入れるかを判定する
struct Fwd<'a> {
    w: Raw,
    /// 残りの整数部の桁数
    int_left: u32,
    /// 今のグループの残り桁数
    grp_left: u32,
    primary: u32,
    secondary: u32,
    has_frac: bool,
    group: &'a [u8],
    decimal: &'a [u8],
}

impl Fwd<'_> {
    #[inline(always)]
    unsafe fn digit(&mut self, c: u8) {
        self.w.b(c);
        if self.int_left > 0 {
            self.int_left -= 1;
            self.grp_left -= 1;
            if self.int_left == 0 {
                if self.has_frac {
                    self.w.s(self.decimal);
                }
            } else if self.grp_left == 0 {
                self.w.s(self.group);
                self.grp_left = if self.int_left == self.primary {
                    self.primary
                } else {
                    self.secondary
                };
            }
        }
    }
    #[inline(always)]
    unsafe fn pair(&mut self, i: u32) {
        let i = i as usize * 2;
        self.digit(LUT[i]);
        self.digit(LUT[i + 1]);
    }
    /// v < 10^4 を先頭の 0 なしで（branchlut の if (v >= 1000) ... の分岐）
    #[inline(always)]
    unsafe fn lead4(&mut self, v: u32) {
        let (d1, d2) = ((v / 100) as usize * 2, (v % 100) as usize * 2);
        if v >= 1000 {
            self.digit(LUT[d1]);
        }
        if v >= 100 {
            self.digit(LUT[d1 + 1]);
        }
        if v >= 10 {
            self.digit(LUT[d2]);
        }
        self.digit(LUT[d2 + 1]);
    }
    #[inline(always)]
    unsafe fn full4(&mut self, v: u32) {
        self.pair(v / 100);
        self.pair(v % 100);
    }
    #[inline(always)]
    unsafe fn lead8(&mut self, v: u32) {
        if v < 10_000 {
            self.lead4(v)
        } else {
            self.lead4(v / 10_000);
            self.full4(v % 10_000);
        }
    }
    #[inline(always)]
    unsafe fn full8(&mut self, v: u32) {
        self.full4(v / 10_000);
        self.full4(v % 10_000);
    }
    /// branchlut の u64 の分岐（10^8, 10^16 で 3 通り）で、上の桁から書く
    #[inline(always)]
    unsafe fn u64(&mut self, v: u64) {
        if v < 100_000_000 {
            self.lead8(v as u32)
        } else if v < 10_000_000_000_000_000 {
            self.lead8((v / 100_000_000) as u32);
            self.full8((v % 100_000_000) as u32);
        } else {
            self.lead4((v / 10_000_000_000_000_000) as u32);
            let r = v % 10_000_000_000_000_000;
            self.full8((r / 100_000_000) as u32);
            self.full8((r % 100_000_000) as u32);
        }
    }
}

/// u32 / u64 それぞれの本体を作る
macro_rules! bodies {
    ($grouped:ident, $t:ty) => {
        /// 区切りあり。P = 第1グループ幅, S = 第2グループ幅, D = 小数桁数
        #[inline(always)]
        unsafe fn $grouped<const P: usize, const S: usize, const D: usize>(
            w: &mut Raw,
            m: $t,
            mg: u32,
            g: &[u8],
            dec: &[u8],
        ) {
            let pd = POW10[D] as $t;
            let int = m / pd;
            let frac = (m % pd) as u32;
            if int as u64 >= POW10[P + mg as usize - 1] {
                let low = (int % POW10[P] as $t) as u32;
                let mut rest = int / POW10[P] as $t;
                let (mut ch, mut k) = ([0u32; 10], 0usize);
                while rest >= POW10[S] as $t {
                    ch[k] = (rest % POW10[S] as $t) as u32;
                    rest /= POW10[S] as $t;
                    k += 1;
                }
                w.lead4(rest as u32);
                while k > 0 {
                    k -= 1;
                    w.s(g);
                    w.fixed::<S>(ch[k]);
                }
                w.s(g);
                w.fixed::<P>(low);
            } else if MIN_GROUP3 && int >= 10_000 {
                // 区切らない整数部は 10^(P + mg - 1) 未満。P = 3、mg = 3 のときだけ 5 桁になりうる
                w.b(b'0' + (int / 10_000) as u8);
                w.fixed::<4>((int % 10_000) as u32);
            } else {
                w.lead4(int as u32);
            }
            if D > 0 {
                w.s(dec);
                w.fixed::<D>(frac);
            }
        }
    };
}
bodies!(grouped64, u64);
bodies!(grouped32, u32);

#[cfg(test)]
mod tests {
    use super::*;

    fn kernel(p: u8, s: u8, mg: u8, d: u8, mask: u16) -> Kernel {
        Layout::new(".", ",", ["", "", "-", ""], p, s, mg, d, mask)
            .unwrap()
            .kernel
    }

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
        let masks = [0u16, 0b101, 0b1101, 0x1ff, 0b1_0000_0010];
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
                    for mask in masks {
                        assert_eq!(
                            kernel(p, s, mg, d, mask),
                            expected(p, s, mg, d, mask),
                            "{p},{s} mg={mg} d={d} mask={mask:b}"
                        );
                    }
                }
            }
        }
    }
}
