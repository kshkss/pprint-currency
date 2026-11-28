//! hypertext の [`Renderable`] 対応（feature "hypertext"）
//!
//! 書式を [`NumberFormat::encode_html`] / [`CurrencyFormat::encode_html`] で HTML 用に変換しておき、
//! `with_i32` / `with_i64` で値と組にすると、テンプレートのバッファへ直接書き出せる。

use alloc::borrow::Cow;
use core::marker::PhantomData;

use hypertext::{Buffer, Renderable, context::Node};

use crate::{CurrencyFormat, Encoding, NumberFormat, affix_len, max_len, sealed};

/// 部品を HTML（本文）向けにエスケープ済み。[`NumberFormat::encode_html`] / [`CurrencyFormat::encode_html`] で作る
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Html;
impl sealed::Sealed for Html {}
impl Encoding for Html {}

/// HTML 用の書式と値の組。hypertext のテンプレートにそのまま埋め込める
///
/// ```
/// use hypertext::prelude::*;
/// use pprint_currency::{CurrencyFormat, CurrencyParams};
///
/// let jpy = CurrencyFormat::new(CurrencyParams {
///     decimal: ".".into(), group: ",".into(), grouping: 3.into(), min_grouping: 1, frac_digits: 0,
///     positive_prefix: "￥".into(), positive_suffix: "".into(),
///     negative_prefix: "-￥".into(), negative_suffix: "".into(),
/// }).unwrap().encode_html();
/// let html = maud! { td { (jpy.with_i64(-1234567)) } }.render();
/// assert_eq!(html.as_inner(), "<td>-￥1,234,567</td>");
/// ```
#[derive(Debug, Clone, Copy)]
pub struct Formatted<'f, F, T> {
    format: &'f F,
    value: T,
}

/// 部品を hypertext と同じ規則（本文）でエスケープする。変わらなければそのまま返す
fn escape(s: Cow<'_, str>) -> Cow<'_, str> {
    let escaped = <str as Renderable<Node>>::render(&s).into_inner();
    // エスケープは文字を長くするだけなので、長さが同じなら何も変わっていない
    if escaped.len() == s.len() {
        s
    } else {
        Cow::Owned(escaped)
    }
}

impl<'a> NumberFormat<'a> {
    /// 部品（小数点・区切り・負符号）を HTML エスケープした書式に変換する
    pub fn encode_html(self) -> NumberFormat<'a, Html> {
        let (decimal, group, minus_sign) = (
            escape(self.decimal),
            escape(self.group),
            escape(self.minus_sign),
        );
        NumberFormat {
            max_len: max_len(minus_sign.len(), &group, &decimal),
            decimal,
            group,
            minus_sign,
            primary: self.primary,
            secondary: self.secondary,
            min_grouping: self.min_grouping,
            frac: self.frac,
            kernel: self.kernel,
            encoding: PhantomData,
        }
    }
}

impl<'a> CurrencyFormat<'a> {
    /// 部品（小数点・区切り・前後の文字列）を HTML エスケープした書式に変換する
    pub fn encode_html(self) -> CurrencyFormat<'a, Html> {
        let (decimal, group) = (escape(self.decimal), escape(self.group));
        let [np, ns] = self.nonneg_affix.map(escape);
        let [gp, gs] = self.neg_affix.map(escape);
        CurrencyFormat {
            max_len: max_len(affix_len([&np, &ns], [&gp, &gs]), &group, &decimal),
            decimal,
            group,
            nonneg_affix: [np, ns],
            neg_affix: [gp, gs],
            primary: self.primary,
            secondary: self.secondary,
            min_grouping: self.min_grouping,
            frac: self.frac,
            kernel: self.kernel,
            encoding: PhantomData,
        }
    }
}

macro_rules! impl_renderable {
    ($($Format:ident)*) => {$(
        impl<'a> $Format<'a, Html> {
            /// 値（小数桁数をスケールとする整数）と組にして、テンプレートに埋め込めるようにする
            pub fn with_i32(&self, value: i32) -> Formatted<'_, Self, i32> {
                Formatted { format: self, value }
            }

            /// 値（小数桁数をスケールとする整数）と組にして、テンプレートに埋め込めるようにする
            pub fn with_i64(&self, value: i64) -> Formatted<'_, Self, i64> {
                Formatted { format: self, value }
            }
        }

        impl Renderable for Formatted<'_, $Format<'_, Html>, i32> {
            #[inline]
            fn render_to(&self, buffer: &mut Buffer) {
                // XSS SAFETY: 数字は ASCII（0-9）で、ほかの部品は encode_html でエスケープ済み。
                // Formatted は Html の書式からしか作れない
                self.format.write_i32(buffer.dangerously_get_string(), self.value);
            }
        }

        impl Renderable for Formatted<'_, $Format<'_, Html>, i64> {
            #[inline]
            fn render_to(&self, buffer: &mut Buffer) {
                // XSS SAFETY: i32 と同じ
                self.format.write_i64(buffer.dangerously_get_string(), self.value);
            }
        }
    )*};
}

impl_renderable!(NumberFormat CurrencyFormat);
