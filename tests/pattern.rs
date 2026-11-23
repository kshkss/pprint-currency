//! 書式文字列（CLDR と同じ構文）から作る書式
use pprint_currency::{
    CurrencyFormat, CurrencyParams, CurrencyPattern, FormatError, NumberFormat, NumberParams,
    NumberPattern, PatternError,
};

fn cur<'a>(
    pattern: &'a str,
    symbol: &'a str,
    decimal: &'a str,
    group: &'a str,
    frac_digits: u8,
) -> CurrencyPattern<'a> {
    CurrencyPattern {
        pattern,
        currency_symbol: symbol.into(),
        decimal: decimal.into(),
        group: group.into(),
        minus_sign: "-".into(),
        min_grouping: 1,
        frac_digits,
    }
}

fn num(pattern: &str, frac_digits: u8) -> NumberPattern<'_> {
    NumberPattern {
        pattern,
        decimal: ".".into(),
        group: ",".into(),
        minus_sign: "-".into(),
        min_grouping: 1,
        frac_digits,
    }
}

fn fmt_cur(p: CurrencyPattern, v: i64) -> String {
    CurrencyFormat::from_pattern(p).unwrap().format_i64(v)
}

fn cur_err(p: CurrencyPattern) -> PatternError {
    CurrencyFormat::from_pattern(p).unwrap_err()
}

fn num_err(p: NumberPattern) -> PatternError {
    NumberFormat::from_pattern(p).unwrap_err()
}

fn unexpected(ch: char, pos: usize) -> PatternError {
    PatternError::Unexpected { ch, pos }
}

#[test]
fn currency_prefix_symbol() {
    let p = || cur("¤#,##0.00", "￥", ".", ",", 0);
    assert_eq!(fmt_cur(p(), -1234567), "-￥1,234,567");
    assert_eq!(fmt_cur(p(), 1000), "￥1,000");
}

#[test]
fn currency_suffix_symbol_keeps_space_in_pattern() {
    let p = || cur("#,##0.00\u{a0}¤", "€", ",", ".", 2);
    assert_eq!(fmt_cur(p(), -123456), "-1.234,56\u{a0}€");
    assert_eq!(fmt_cur(p(), 5), "0,05\u{a0}€");
}

#[test]
fn currency_explicit_negative_subpattern() {
    let p = || cur("¤#,##0.00;(¤#,##0.00)", "$", ".", ",", 2);
    assert_eq!(fmt_cur(p(), -123456), "($1,234.56)");
    assert_eq!(fmt_cur(p(), 123456), "$1,234.56");
}

#[test]
fn currency_indian_grouping() {
    let p = cur("¤#,##,##0.00", "₹", ".", ",", 2);
    assert_eq!(fmt_cur(p, 123456700), "₹12,34,567.00");
}

#[test]
fn currency_negative_subpattern_with_different_spacing() {
    let p = || cur("¤ #,##0.00;¤-#,##0.00", "CHF", ".", "’", 2);
    assert_eq!(fmt_cur(p(), 123456), "CHF 1’234.56");
    assert_eq!(fmt_cur(p(), -123456), "CHF-1’234.56");
}

#[test]
fn currency_symbol_is_inserted_as_is() {
    assert_eq!(
        fmt_cur(cur("¤#,##0.00", "CHF", ".", ",", 2), 100),
        "CHF1.00"
    );
    assert_eq!(
        fmt_cur(cur("¤#,##0.00", "CHF\u{a0}", ".", ",", 2), 100),
        "CHF\u{a0}1.00"
    );
}

#[test]
fn currency_sign_run_is_one_symbol() {
    let want = fmt_cur(cur("¤#,##0.00", "$", ".", ",", 2), -123456);
    for pat in ["¤¤#,##0.00", "¤¤¤#,##0.00", "¤¤¤¤¤#,##0.00"] {
        assert_eq!(fmt_cur(cur(pat, "$", ".", ",", 2), -123456), want, "{pat}");
    }
}

#[test]
fn quoted_literals() {
    assert_eq!(fmt_cur(cur("'x'#,##0", "$", ".", ",", 0), -5), "-x5");
    assert_eq!(fmt_cur(cur("#,##0''", "$", ".", ",", 0), 5), "5'");
    assert_eq!(fmt_cur(cur("'a;b'#,##0", "$", ".", ",", 0), 5), "a;b5");
    assert_eq!(fmt_cur(cur("#,##0'%'", "$", ".", ",", 0), 5), "5%");
    assert_eq!(
        fmt_cur(cur("'o''clock'#,##0", "$", ".", ",", 0), 5),
        "o'clock5"
    );
}

#[test]
fn number_pattern() {
    let f = NumberFormat::from_pattern(num("#,##0.###", 2)).unwrap();
    assert_eq!(f.format_i64(-123456), "-1,234.56");
    let f = NumberFormat::from_pattern(NumberPattern {
        minus_sign: "\u{2212}".into(),
        ..num("#,##0.###", 0)
    })
    .unwrap();
    assert_eq!(f.format_i64(-1234), "\u{2212}1,234");
}

#[test]
fn number_pattern_rejects_affixes_and_subpatterns() {
    assert_eq!(num_err(num("¤#,##0.###", 0)), unexpected('¤', 0));
    assert_eq!(num_err(num("#,##0.### 'x'", 0)), unexpected(' ', 9));
    assert_eq!(num_err(num("#,##0.###;-#,##0.###", 0)), unexpected(';', 9));
    assert_eq!(num_err(num("-#,##0", 0)), unexpected('-', 0));
}

#[test]
fn same_output_as_params() {
    let vals = [
        0i64,
        1,
        -1,
        5,
        -5,
        999,
        1000,
        -1000,
        1234567,
        -1234567,
        i64::MAX,
        i64::MIN,
    ];
    for (pat, frac) in [("¤#,##0.00", 2u8), ("#,##,##0.00 ¤;(¤#,##0.00)", 3)] {
        let p = cur(pat, "$", ",", ".", frac);
        let from_pattern = CurrencyFormat::from_pattern(p).unwrap();
        let (pp, ps, np, ns) = if pat.starts_with('¤') {
            ("$", "", "-$", "")
        } else {
            ("", " $", "($", ")")
        };
        let g = if pat.starts_with('¤') {
            3.into()
        } else {
            (3, 2).into()
        };
        let from_params = CurrencyFormat::new(CurrencyParams {
            decimal: ",".into(),
            group: ".".into(),
            grouping: g,
            min_grouping: 1,
            frac_digits: frac,
            positive_prefix: pp.into(),
            positive_suffix: ps.into(),
            negative_prefix: np.into(),
            negative_suffix: ns.into(),
        })
        .unwrap();
        for v in vals {
            assert_eq!(
                from_pattern.format_i64(v),
                from_params.format_i64(v),
                "{pat} {v}"
            );
        }
    }
    let from_pattern = NumberFormat::from_pattern(num("#,##0.###", 3)).unwrap();
    let from_params = NumberFormat::new(NumberParams {
        decimal: ".".into(),
        group: ",".into(),
        grouping: 3.into(),
        min_grouping: 1,
        frac_digits: 3,
        minus_sign: "-".into(),
    })
    .unwrap();
    for v in vals {
        assert_eq!(from_pattern.format_i64(v), from_params.format_i64(v), "{v}");
    }
}

#[test]
fn rejects_uninterpretable_chars() {
    let e = |pat| cur_err(cur(pat, "$", ".", ",", 2));
    assert_eq!(e("#,##0%"), unexpected('%', 5));
    assert_eq!(e("#,##0.00‰"), unexpected('‰', 8));
    assert_eq!(e("*x#,##0"), unexpected('*', 0));
    assert_eq!(e("+#,##0"), unexpected('+', 0));
    assert_eq!(e("@@#"), unexpected('@', 0));
    assert_eq!(e("#,##0E0"), unexpected('E', 5));
    assert_eq!(e("#,##0.0,0"), unexpected(',', 7));
    assert_eq!(e("#,##5"), unexpected('5', 4));
    assert_eq!(e("#,##0.0.0"), unexpected('.', 7));
    assert_eq!(e("¤#,##0;-¤#,##0;x"), unexpected(';', 16));
}

#[test]
fn rejects_other_errors() {
    let e = |pat| cur_err(cur(pat, "$", ".", ",", 2));
    assert_eq!(e("'x#,##0"), PatternError::UnterminatedQuote);
    assert_eq!(e("¤"), PatternError::NoDigits);
    assert_eq!(e(""), PatternError::NoDigits);
    assert_eq!(num_err(num("", 0)), PatternError::NoDigits);
    assert_eq!(e("#0.###"), PatternError::Format(FormatError::GroupingZero));
    assert_eq!(
        num_err(num("#,##0", 20)),
        PatternError::Format(FormatError::FracDigitsTooLarge)
    );
}
