//! 書き出しの本体。
//! 桁数の判定と数字の書き出しは miloyip/itoa-benchmark の branchlut（MIT、Copyright (C) 2014 Milo Yip）を
//! ベースにしている（README の Acknowledgements を参照）。
//! 固定小数点の整数を、グループ単位（2 / 2+1 / 2+2 … 桁）で
//! 区切りの判定なしに書き出す（FGR 方式）。数字は ASCII のみ。

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
pub(crate) const GROUP3: bool = cfg!(feature = "group-3");
/// インド式（3, 2）
pub(crate) const INDIAN: bool = cfg!(feature = "group-indian");
/// 2 桁区切り（2, 2）
pub(crate) const GROUP2: bool = cfg!(feature = "group-2");
/// 速い経路で扱う minimumGroupingDigits
pub(crate) const MIN_GROUP1: bool = cfg!(feature = "min-group-1");
pub(crate) const MIN_GROUP2: bool = cfg!(feature = "min-group-2");
/// 3 を扱うと、3 桁区切りで区切らない整数部が 5 桁になりうる
pub(crate) const MIN_GROUP3: bool = cfg!(feature = "min-group-3");

/// どの書き出し方を使うか。構築時に一度だけ決める
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kernel {
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

/// 生ポインタへの書き込み（容量確認なし。呼び出し側で reserve 済み）
pub(crate) struct Raw {
    pub(crate) p: *mut u8,
}

impl Raw {
    #[inline(always)]
    unsafe fn b(&mut self, x: u8) {
        *self.p = x;
        self.p = self.p.add(1);
    }
    #[inline(always)]
    pub(crate) unsafe fn s(&mut self, s: &[u8]) {
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
pub(crate) fn count_digits(v: u64) -> u32 {
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
pub(crate) struct Fwd<'a> {
    pub(crate) w: Raw,
    /// 残りの整数部の桁数
    pub(crate) int_left: u32,
    /// 今のグループの残り桁数
    pub(crate) grp_left: u32,
    pub(crate) primary: u32,
    pub(crate) secondary: u32,
    pub(crate) has_frac: bool,
    pub(crate) group: &'a [u8],
    pub(crate) decimal: &'a [u8],
}

impl Fwd<'_> {
    #[inline(always)]
    pub(crate) unsafe fn digit(&mut self, c: u8) {
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
    pub(crate) unsafe fn u64(&mut self, v: u64) {
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
        pub(crate) unsafe fn $grouped<const P: usize, const S: usize, const D: usize>(
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
