//! 書式文字列（UTS #35 / ICU の DecimalFormat パターンと同じ構文）から書式を作る

use std::borrow::Cow;

use crate::{CurrencyFormat, CurrencyParams, FormatError, Grouping, NumberFormat, NumberParams};

/// 数値の書式文字列と、そこに入れる記号・値
///
/// 書式文字列の中の `.` と `,` はプレースホルダで、それぞれ `decimal`（小数点）と `group`（桁区切り）に置き換わる。
/// 数値の書式文字列に書けるのは数値部（例: `"#,##0.###"`）だけで、前後の文字や負のサブパターン（`;` 以降）は書けない。
/// 負の数は `minus_sign` + 数値部になる。
///
/// ```
/// use pprint_currency::{NumberFormat, NumberPattern};
///
/// let de = NumberFormat::from_pattern(NumberPattern {
///     pattern: "#,##0.###",
///     decimal: ",".into(), group: ".".into(), minus_sign: "-".into(),
///     min_grouping: 1, frac_digits: 2,
/// }).unwrap();
/// assert_eq!(de.format_i64(-123456), "-1.234,56");
/// ```
#[derive(Debug, Clone)]
pub struct NumberPattern<'a> {
    /// 書式文字列（例: `"#,##0.###"`）。区切り方を決める。小数部の桁数は使わない（`frac_digits` を使う）
    pub pattern: &'a str,
    /// `.` の位置に入れる小数点
    pub decimal: Cow<'a, str>,
    /// `,` の位置に入れる桁区切り
    pub group: Cow<'a, str>,
    /// 負の数の先頭に付ける負符号
    pub minus_sign: Cow<'a, str>,
    /// 整数部がこの桁数＋第1グループの幅以上のときだけ区切る（CLDR の minimumGroupingDigits。通常 1）
    pub min_grouping: u8,
    /// 小数桁数（値のスケール）
    pub frac_digits: u8,
}

/// 通貨の書式文字列と、そこに入れる記号・値
///
/// 書式文字列の中の `.`・`,`・`-`・`¤` はプレースホルダで、それぞれ `decimal`（小数点）・`group`（桁区切り）・
/// `minus_sign`（負符号）・`currency_symbol`（通貨記号）に置き換わる。
/// 空白を含め、それ以外の文字は書いたとおりに出る（`'...'` で囲んだ文字も同じ。`''` は `'` 1 文字）。
///
/// - `;` の後に負の数の書式を書ける（例: `"¤#,##0.00;(¤#,##0.00)"`）。
///   無ければ負の数は `-` + 正の数の書式になる。負の数の書式からは前後の文字だけを使い、区切り方は正の数の書式から取る
/// - `¤` がいくつ続いても（`¤¤` など）1 つの通貨記号として扱う
/// - `¤` には `currency_symbol` をそのまま入れる。記号と数字の間に空白を自動で足すことはしない
///   （CLDR の currencySpacing は扱わない）。空白が要るなら、書式文字列か `currency_symbol` に含める
///
/// ```
/// use pprint_currency::{CurrencyFormat, CurrencyPattern};
///
/// let chf = CurrencyFormat::from_pattern(CurrencyPattern {
///     pattern: "¤ #,##0.00;¤-#,##0.00",
///     currency_symbol: "CHF".into(),
///     decimal: ".".into(), group: "’".into(), minus_sign: "-".into(),
///     min_grouping: 1, frac_digits: 2,
/// }).unwrap();
/// assert_eq!(chf.format_i64(123456), "CHF 1’234.56");
/// assert_eq!(chf.format_i64(-123456), "CHF-1’234.56");
/// ```
#[derive(Debug, Clone)]
pub struct CurrencyPattern<'a> {
    /// 書式文字列（例: `"#,##0.00 ¤"`、`"¤#,##0.00;(¤#,##0.00)"`）。小数部の桁数は使わない（`frac_digits` を使う）
    pub pattern: &'a str,
    /// `¤` の位置にそのまま入れる通貨記号
    pub currency_symbol: Cow<'a, str>,
    /// `.` の位置に入れる小数点
    pub decimal: Cow<'a, str>,
    /// `,` の位置に入れる桁区切り
    pub group: Cow<'a, str>,
    /// `-` の位置に入れる負符号
    pub minus_sign: Cow<'a, str>,
    /// 整数部がこの桁数＋第1グループの幅以上のときだけ区切る（CLDR の minimumGroupingDigits。通常 1）
    pub min_grouping: u8,
    /// 小数桁数（その通貨の値のスケール）
    pub frac_digits: u8,
}

/// 書式文字列から書式を作れなかったときのエラー
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternError {
    /// 解釈できない文字（`pos` は書式文字列の中のバイト位置）
    Unexpected { ch: char, pos: usize },
    /// 閉じていない `'`
    UnterminatedQuote,
    /// 数値部（`0` を含む `#,##0` のような部分）が無い
    NoDigits,
    /// 書式の値が不正
    Format(FormatError),
}

impl std::fmt::Display for PatternError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatternError::Unexpected { ch, pos } => {
                write!(f, "書式文字列の {pos} バイト目の {ch:?} を解釈できません")
            }
            PatternError::UnterminatedQuote => f.write_str("書式文字列の ' が閉じていません"),
            PatternError::NoDigits => f.write_str("書式文字列に数値部（#,##0 など）がありません"),
            PatternError::Format(e) => e.fmt(f),
        }
    }
}

impl std::error::Error for PatternError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            PatternError::Format(e) => Some(e),
            _ => None,
        }
    }
}

impl From<FormatError> for PatternError {
    fn from(e: FormatError) -> Self {
        PatternError::Format(e)
    }
}

impl<'a> NumberFormat<'a> {
    /// 書式文字列から書式を作る（書式文字列の意味は [`NumberPattern`] を参照）
    pub fn from_pattern(p: NumberPattern<'a>) -> Result<Self, PatternError> {
        let mut s = Scanner::new(p.pattern);
        let grouping = s.number()?;
        if let Some((pos, ch)) = s.peek() {
            return Err(PatternError::Unexpected { ch, pos });
        }
        Ok(NumberFormat::new(NumberParams {
            decimal: p.decimal,
            group: p.group,
            grouping,
            min_grouping: p.min_grouping,
            frac_digits: p.frac_digits,
            minus_sign: p.minus_sign,
        })?)
    }
}

impl<'a> CurrencyFormat<'a> {
    /// 書式文字列から書式を作る（書式文字列の意味は [`CurrencyPattern`] を参照）
    pub fn from_pattern(p: CurrencyPattern<'a>) -> Result<Self, PatternError> {
        let (symbol, minus) = (&*p.currency_symbol, &*p.minus_sign);
        let mut s = Scanner::new(p.pattern);
        let pos_prefix = s.affix(symbol, minus, Side::Prefix)?;
        let grouping = s.number()?;
        let pos_suffix = s.affix(symbol, minus, Side::Suffix)?;
        let (neg_prefix, neg_suffix) = if s.peek().is_some() {
            s.next(); // ';'
            let prefix = s.affix(symbol, minus, Side::Prefix)?;
            s.number()?; // 区切り方は正の数の書式から取る（UTS #35）
            let suffix = s.affix(symbol, minus, Side::Suffix)?;
            if let Some((pos, ch)) = s.peek() {
                return Err(PatternError::Unexpected { ch, pos });
            }
            (prefix, suffix)
        } else {
            (format!("{minus}{pos_prefix}"), pos_suffix.clone())
        };
        Ok(CurrencyFormat::new(CurrencyParams {
            decimal: p.decimal,
            group: p.group,
            grouping,
            min_grouping: p.min_grouping,
            frac_digits: p.frac_digits,
            positive_prefix: pos_prefix.into(),
            positive_suffix: pos_suffix.into(),
            negative_prefix: neg_prefix.into(),
            negative_suffix: neg_suffix.into(),
        })?)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Side {
    Prefix,
    Suffix,
}

/// 構文上の意味を持つ文字（UTS #35）。前後の文字の中で、これ以外は書いたとおりの文字として扱う
fn is_special(c: char) -> bool {
    matches!(
        c,
        '0'..='9' | '@' | '#' | '.' | ',' | 'E' | '+' | '%' | '‰' | '*' | '\'' | ';' | '¤' | '-'
    )
}

struct Scanner<'s> {
    src: &'s str,
    pos: usize,
}

impl<'s> Scanner<'s> {
    fn new(src: &'s str) -> Self {
        Scanner { src, pos: 0 }
    }

    fn peek(&self) -> Option<(usize, char)> {
        self.src[self.pos..].chars().next().map(|c| (self.pos, c))
    }

    fn next(&mut self) -> Option<(usize, char)> {
        let r = self.peek();
        if let Some((_, c)) = r {
            self.pos += c.len_utf8();
        }
        r
    }

    /// 前後の文字を、記号を置き換えながら読む。
    /// 前の文字は数値部（`#` `0` `,`）の手前まで、後の文字は `;` か終わりまで
    fn affix(&mut self, symbol: &str, minus: &str, side: Side) -> Result<String, PatternError> {
        let mut out = String::new();
        // 数値部の直後の E は指数の書式
        if side == Side::Suffix {
            if let Some((pos, 'E')) = self.peek() {
                return Err(PatternError::Unexpected { ch: 'E', pos });
            }
        }
        while let Some((pos, c)) = self.peek() {
            match c {
                '#' | '0' | ',' if side == Side::Prefix => return Ok(out),
                ';' if side == Side::Suffix => return Ok(out),
                '\'' => {
                    self.next();
                    self.quoted(&mut out)?;
                }
                '¤' => {
                    while let Some((_, '¤')) = self.peek() {
                        self.next();
                    }
                    out.push_str(symbol);
                }
                '-' => {
                    self.next();
                    out.push_str(minus);
                }
                c if is_special(c) => return Err(PatternError::Unexpected { ch: c, pos }),
                c => {
                    self.next();
                    out.push(c);
                }
            }
        }
        match side {
            Side::Prefix => Err(PatternError::NoDigits),
            Side::Suffix => Ok(out),
        }
    }

    /// 開きの `'` の後から読む。`''` は `'` 1 文字
    fn quoted(&mut self, out: &mut String) -> Result<(), PatternError> {
        if let Some((_, '\'')) = self.peek() {
            self.next();
            out.push('\'');
            return Ok(());
        }
        loop {
            match self.next() {
                None => return Err(PatternError::UnterminatedQuote),
                Some((_, '\'')) => {
                    if let Some((_, '\'')) = self.peek() {
                        self.next();
                        out.push('\'');
                    } else {
                        return Ok(());
                    }
                }
                Some((_, c)) => out.push(c),
            }
        }
    }

    /// 数値部（`[#,]* 0 [0,]* ('.' 0* #*)?`）を読み、グループ幅を返す
    fn number(&mut self) -> Result<Grouping, PatternError> {
        let start = self.pos;
        let mut seen_zero = false;
        while let Some((pos, c)) = self.peek() {
            match c {
                '#' if !seen_zero => {}
                '0' => seen_zero = true,
                ',' => {}
                '#' | '1'..='9' | '@' => return Err(PatternError::Unexpected { ch: c, pos }),
                _ => break,
            }
            self.next();
        }
        if !seen_zero {
            return Err(match self.peek() {
                Some((pos, ch)) => PatternError::Unexpected { ch, pos },
                None => PatternError::NoDigits,
            });
        }
        let integer = &self.src[start..self.pos];
        if let Some((_, '.')) = self.peek() {
            self.next();
            let mut seen_hash = false;
            while let Some((pos, c)) = self.peek() {
                match c {
                    '0' if !seen_hash => {}
                    '#' => seen_hash = true,
                    '0'..='9' | ',' | '.' | '@' => {
                        return Err(PatternError::Unexpected { ch: c, pos })
                    }
                    _ => break,
                }
                self.next();
            }
        }
        Ok(grouping(integer))
    }
}

/// 整数部（例: `"#,##,##0"`）のグループ幅。`,` が無ければ 0（区切らない）
fn grouping(integer: &str) -> Grouping {
    let parts: Vec<&str> = integer.split(',').collect();
    let n = parts.len();
    if n < 2 {
        return Grouping::new(0, 0);
    }
    let width = |s: &str| u8::try_from(s.len()).unwrap_or(u8::MAX);
    let primary = width(parts[n - 1]);
    let secondary = if n >= 3 { width(parts[n - 2]) } else { primary };
    Grouping::new(primary, secondary)
}
