//! tools/gen.py が CLDR から作った値（tests/cldr_params.json）で書式を作り、
//! Node の Intl.NumberFormat の出力（tools/golden.js で生成した tests/golden.json）と照合する。
//! どちらも生成物なのでリポジトリには含めない。無ければこのテストは何もせずに終わる（README の「テスト」を参照）
use pprint_currency::{CurrencyFormat, CurrencyParams, NumberFormat, NumberParams};
use serde_json::Value;

fn json(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// 照合用データ。無ければ None（理由を表示する）
fn data() -> Option<(Value, Value)> {
    let (p, g) = ("tests/cldr_params.json", "tests/golden.json");
    if !std::path::Path::new(p).exists() || !std::path::Path::new(g).exists() {
        eprintln!("{p} / {g} が無いので Intl との照合を省略します（tools/gen.py と tools/golden.js で生成できます）");
        return None;
    }
    Some((json(p), json(g)))
}
fn st(v: &Value, k: &str) -> &'static str {
    // テストの間だけ使う値なので、'static にするために leak する
    Box::leak(v[k].as_str().unwrap().to_owned().into_boxed_str())
}
fn n8(v: &Value, k: &str) -> u8 {
    v[k].as_u64().unwrap() as u8
}

fn currency(p: &Value, l: &str, code: &str) -> CurrencyFormat<'static> {
    let c = &p[l]["currencies"][code];
    CurrencyFormat::new(CurrencyParams {
        decimal: st(c, "decimal"),
        group: st(c, "group"),
        primary_grouping: n8(c, "primary_grouping"),
        secondary_grouping: n8(c, "secondary_grouping"),
        min_grouping: n8(c, "min_grouping"),
        frac_digits: n8(c, "frac_digits"),
        positive_prefix: st(c, "positive_prefix"),
        positive_suffix: st(c, "positive_suffix"),
        negative_prefix: st(c, "negative_prefix"),
        negative_suffix: st(c, "negative_suffix"),
    })
    .unwrap()
}

fn number(p: &Value, l: &str, d: u8) -> NumberFormat<'static> {
    let c = &p[l]["number"];
    NumberFormat::new(NumberParams {
        decimal: st(c, "decimal"),
        group: st(c, "group"),
        primary_grouping: n8(c, "primary_grouping"),
        secondary_grouping: n8(c, "secondary_grouping"),
        min_grouping: n8(c, "min_grouping"),
        frac_digits: d,
        positive_prefix: st(c, "positive_prefix"),
        positive_suffix: st(c, "positive_suffix"),
        negative_prefix: st(c, "negative_prefix"),
        negative_suffix: st(c, "negative_suffix"),
    })
    .unwrap()
}

#[test]
fn currency_matches_intl() {
    let Some((p, g)) = data() else { return };
    let mut n = 0;
    let mut cache: Option<(String, String, CurrencyFormat)> = None;
    for row in g["currency"].as_array().unwrap() {
        let (l, code, m, want) = (
            row[0].as_str().unwrap(),
            row[1].as_str().unwrap(),
            row[2].as_str().unwrap(),
            row[3].as_str().unwrap(),
        );
        if cache.as_ref().is_none_or(|c| c.0 != l || c.1 != code) {
            cache = Some((l.into(), code.into(), currency(&p, l, code)));
        }
        let f = &cache.as_ref().unwrap().2;
        let v: i64 = m.parse().unwrap();
        let mut s = String::from("既存");
        f.write_i64(&mut s, v);
        assert_eq!(&s["既存".len()..], want, "{l} {code} {v}");
        if let Ok(v32) = i32::try_from(v) {
            assert_eq!(f.format_i32(v32), want, "i32 {l} {code} {v}");
        }
        n += 1;
    }
    println!("currency: {n} 件一致");
}

#[test]
fn number_matches_intl() {
    let Some((p, g)) = data() else { return };
    let mut n = 0;
    for row in g["number"].as_array().unwrap() {
        let (l, d, m, want) = (
            row[0].as_str().unwrap(),
            row[1].as_u64().unwrap() as u8,
            row[2].as_str().unwrap(),
            row[3].as_str().unwrap(),
        );
        let f = number(&p, l, d);
        let v: i64 = m.parse().unwrap();
        assert_eq!(f.format_i64(v), want, "{l} d={d} {v}");
        if let Ok(v32) = i32::try_from(v) {
            assert_eq!(f.format_i32(v32), want, "i32 {l} d={d} {v}");
        }
        n += 1;
    }
    println!("number: {n} 件一致");
}
