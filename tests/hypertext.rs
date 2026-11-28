//! hypertext の Renderable 対応（feature "hypertext"）
#![cfg(feature = "hypertext")]

use hypertext::{Buffer, prelude::*};
use pprint_currency::{CurrencyFormat, CurrencyParams, Grouping, Html, NumberFormat, NumberParams};

fn number(group: &str, decimal: &str, minus: &str, g: Grouping, d: u8) -> NumberFormat<'static> {
    NumberFormat::new(NumberParams {
        decimal: decimal.to_owned().into(),
        group: group.to_owned().into(),
        grouping: g,
        min_grouping: 1,
        frac_digits: d,
        minus_sign: minus.to_owned().into(),
    })
    .unwrap()
}

fn currency(prefix: &str, suffix: &str, g: Grouping, d: u8) -> CurrencyFormat<'static> {
    CurrencyFormat::new(CurrencyParams {
        decimal: ".".into(),
        group: ",".into(),
        grouping: g,
        min_grouping: 1,
        frac_digits: d,
        positive_prefix: prefix.to_owned().into(),
        positive_suffix: suffix.to_owned().into(),
        negative_prefix: format!("-{prefix}").into(),
        negative_suffix: suffix.to_owned().into(),
    })
    .unwrap()
}

const VALS: [i64; 9] = [
    0,
    5,
    -5,
    1234567,
    -1234567,
    i32::MAX as i64,
    i32::MIN as i64,
    i64::MAX,
    i64::MIN,
];

/// エスケープの要らない部品なら、render の結果は format_* と同じ（速い経路・遅い経路の両方）
#[test]
fn render_matches_format_when_no_escape_needed() {
    // (3, 3)・(3, 2) は速い経路（features が有効なら）、(4, 4) と小数 11 桁は遅い経路
    for (g, d) in [
        (3.into(), 2),
        ((3, 2).into(), 0),
        (4.into(), 2),
        (3.into(), 11),
    ] {
        let num = number(",", ".", "-", g, d);
        let cur = currency("￥", " 円", g, d);
        let (num_h, cur_h) = (num.clone().encode_html(), cur.clone().encode_html());
        for v in VALS {
            let ctx = format!("{g:?} d={d} {v}");
            assert_eq!(
                num_h.with_i64(v).render().into_inner(),
                num.format_i64(v),
                "{ctx}"
            );
            assert_eq!(
                cur_h.with_i64(v).render().into_inner(),
                cur.format_i64(v),
                "{ctx}"
            );
            if let Ok(v) = i32::try_from(v) {
                assert_eq!(
                    num_h.with_i32(v).render().into_inner(),
                    num.format_i32(v),
                    "{ctx}"
                );
                assert_eq!(
                    cur_h.with_i32(v).render().into_inner(),
                    cur.format_i32(v),
                    "{ctx}"
                );
            }
        }
    }
}

/// テンプレートに埋め込める
#[test]
fn embeds_in_templates() {
    let jpy = currency("￥", "", 3.into(), 0).encode_html();
    let de = number(".", ",", "-", 3.into(), 2).encode_html();
    // maud! / rsx! は値を move するクロージャなので、2 回使えるように参照で渡す
    let (jpy, de) = (&jpy, &de);
    let price = -1234567i64;
    let qty = 123456i32;
    let m = maud! { td { (jpy.with_i64(price)) } td { (de.with_i32(qty)) } }.render();
    assert_eq!(m.as_inner(), "<td>-￥1,234,567</td><td>1.234,56</td>");
    let r = rsx! { <td>(jpy.with_i64(price))</td><td>(de.with_i32(qty))</td> }.render();
    assert_eq!(r.as_inner(), m.as_inner());
}

/// 前後の文字列・区切り・小数点・負符号の `& < >` はエスケープされる
#[test]
fn escapes_html_special_chars() {
    let cur = currency("<b>", "&amp", 3.into(), 2).encode_html();
    assert_eq!(
        cur.with_i64(123456).render().into_inner(),
        "&lt;b&gt;1,234.56&amp;amp"
    );
    assert_eq!(
        cur.with_i64(-5).render().into_inner(),
        "-&lt;b&gt;0.05&amp;amp"
    );

    // 区切り・小数点・負符号が特殊文字。区切りの多い長い値でも書き込み量の上限を超えない
    // （超えると debug ビルドの debug_assert! で落ちる）
    for (g, d) in [(3.into(), 2), (1.into(), 0), (1.into(), 19), (4.into(), 3)] {
        let plain = number("&", "<", ">", g, d);
        let html = plain.clone().encode_html();
        for v in VALS {
            let want = plain
                .format_i64(v)
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            assert_eq!(
                html.with_i64(v).render().into_inner(),
                want,
                "{g:?} d={d} {v}"
            );
            // Html の書式でも write_* / format_* は使え、エスケープ済みの文字列になる
            assert_eq!(html.format_i64(v), want, "format {g:?} d={d} {v}");
        }
    }
}

/// render_to は、中身のあるバッファの末尾に追記する
#[test]
fn render_to_appends() {
    let jpy = currency("￥", "", 3.into(), 0).encode_html();
    let slow = number(",", ".", "-", 4.into(), 2).encode_html();
    let mut buf = Buffer::new();
    maud! { p { "合計: " } }.render_to(&mut buf);
    jpy.with_i64(-1234567).render_to(&mut buf);
    slow.with_i32(12345678).render_to(&mut buf);
    jpy.with_i32(0).render_to(&mut buf);
    assert_eq!(
        buf.rendered().into_inner(),
        "<p>合計: </p>-￥1,234,56712,3456.78￥0"
    );
}

/// 型引数の既定値は Plain。Html は明示して書ける
#[test]
fn encoding_type_parameter() {
    let plain: NumberFormat<'static> = number(",", ".", "-", 3.into(), 0);
    let html: NumberFormat<'static, Html> = plain.encode_html();
    assert_eq!(html.format_i64(1000), "1,000");
}
