//! 自分で値を決めた書式（速い経路に当てはまらない組み合わせを含む）を、素朴な参照実装と比べる
use pprint_currency::{
    CurrencyFormat, CurrencyParams, FormatError, Grouping, NumberFormat, NumberParams,
};

/// 参照実装: 10 進文字列を作り、右から 1 文字ずつ数えて区切りを入れる
fn reference(p: &CurrencyParams, v: i64) -> String {
    let d = p.frac_digits as usize;
    let mut s = v.unsigned_abs().to_string();
    while s.len() < d + 1 {
        s.insert(0, '0');
    }
    let (int, frac) = s.split_at(s.len() - d);
    let (pr, sc) = (
        p.grouping.primary() as usize,
        p.grouping.secondary() as usize,
    );
    let mut body = String::new();
    let grouping = int.len() >= pr + p.min_grouping as usize;
    let digits: Vec<char> = int.chars().collect();
    for (i, c) in digits.iter().enumerate() {
        let from_right = digits.len() - i; // この桁を含めて右に残る桁数
        if grouping && i > 0 && from_right >= pr && (from_right - pr).is_multiple_of(sc) {
            body.push_str(&p.group);
        }
        body.push(*c);
    }
    if d > 0 {
        body.push_str(&p.decimal);
        body.push_str(frac);
    }
    if v < 0 {
        format!("{}{}{}", p.negative_prefix, body, p.negative_suffix)
    } else {
        format!("{}{}{}", p.positive_prefix, body, p.positive_suffix)
    }
}

#[test]
fn custom_params_match_reference() {
    let mut x: u64 = 0x9E3779B97F4A7C15;
    let mut vals: Vec<i64> = vec![
        0,
        1,
        -1,
        7,
        i64::MAX,
        i64::MIN,
        i32::MAX as i64,
        i32::MIN as i64,
    ];
    for k in 0..19u32 {
        let p = 10i64.pow(k);
        vals.extend([p, p - 1, -p, p + 1]);
    }
    for _ in 0..300 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        vals.push((x >> (x % 64)) as i64 * if x & 1 == 0 { 1 } else { -1 });
    }
    let mut n = 0;
    let groupings: [Grouping; 10] = [
        3.into(),
        2.into(),
        4.into(),
        1.into(),
        (3, 3).into(),
        (3, 2).into(),
        (2, 2).into(),
        (2, 3).into(),
        (3, 4).into(),
        (4, 3).into(),
    ];
    for g in groupings {
        let (pr, sc) = (g.primary(), g.secondary());
        for mg in [1u8, 2, 3, 4] {
            for d in [0u8, 1, 2, 3, 4, 5, 8, 9, 10, 15, 19] {
                for (group, decimal) in [(",", "."), ("\u{202f}", ","), ("", "")] {
                    let cp = CurrencyParams {
                        decimal: decimal.into(),
                        group: group.into(),
                        grouping: g,
                        min_grouping: mg,
                        frac_digits: d,
                        positive_prefix: "+[".into(),
                        positive_suffix: "]".into(),
                        negative_prefix: "−(".into(),
                        negative_suffix: ") 円".into(),
                    };
                    // 数値は負の値の前に負符号を付けるだけ
                    let np = CurrencyParams {
                        positive_prefix: "".into(),
                        positive_suffix: "".into(),
                        negative_suffix: "".into(),
                        ..cp.clone()
                    };
                    let f = NumberFormat::new(NumberParams {
                        decimal: decimal.into(),
                        group: group.into(),
                        grouping: g,
                        min_grouping: mg,
                        frac_digits: d,
                        minus_sign: cp.negative_prefix.clone(),
                    })
                    .unwrap();
                    let c = CurrencyFormat::new(cp.clone()).unwrap();
                    for &v in &vals {
                        let want_n = reference(&np, v);
                        let want_c = reference(&cp, v);
                        let mut s = String::from("前");
                        f.write_i64(&mut s, v);
                        assert_eq!(&s["前".len()..], want_n, "{pr},{sc} mg={mg} d={d} {v}");
                        assert_eq!(
                            c.format_i64(v),
                            want_c,
                            "currency {pr},{sc} mg={mg} d={d} {v}"
                        );
                        if let Ok(v32) = i32::try_from(v) {
                            assert_eq!(
                                f.format_i32(v32),
                                want_n,
                                "i32 {pr},{sc} mg={mg} d={d} {v}"
                            );
                            assert_eq!(
                                c.format_i32(v32),
                                want_c,
                                "currency i32 {pr},{sc} mg={mg} d={d} {v}"
                            );
                        }
                        n += 1;
                    }
                }
            }
        }
    }
    println!("{n} 件一致");
}

#[test]
fn rejects_invalid_params() {
    let base = NumberParams {
        decimal: ".".into(),
        group: ",".into(),
        grouping: 3.into(),
        min_grouping: 1,
        frac_digits: 2,
        minus_sign: "-".into(),
    };
    assert_eq!(
        NumberFormat::new(NumberParams {
            frac_digits: 20,
            ..base.clone()
        })
        .unwrap_err(),
        FormatError::FracDigitsTooLarge
    );
    assert_eq!(
        NumberFormat::new(NumberParams {
            min_grouping: 0,
            ..base.clone()
        })
        .unwrap_err(),
        FormatError::MinGroupingZero
    );
    for g in [20.into(), (3, 20).into(), (20, 3).into()] {
        assert_eq!(
            NumberFormat::new(NumberParams {
                grouping: g,
                ..base.clone()
            })
            .unwrap_err(),
            FormatError::GroupingTooLarge,
            "{g:?}"
        );
    }
    for g in [0.into(), (0, 3).into(), (3, 0).into()] {
        assert_eq!(
            NumberFormat::new(NumberParams {
                grouping: g,
                ..base.clone()
            })
            .unwrap_err(),
            FormatError::GroupingZero,
            "{g:?}"
        );
    }
    assert_eq!(
        CurrencyFormat::new(CurrencyParams {
            decimal: base.decimal.clone(),
            group: base.group.clone(),
            grouping: (3, 0).into(),
            min_grouping: base.min_grouping,
            frac_digits: base.frac_digits,
            positive_prefix: "".into(),
            positive_suffix: "".into(),
            negative_prefix: base.minus_sign.clone(),
            negative_suffix: "".into(),
        })
        .unwrap_err(),
        FormatError::GroupingZero
    );
}

/// write_* は、中身のある String の末尾に追記し、前の中身を壊さないこと（速い経路・遅い経路、i32・i64 の両方）
#[test]
fn write_appends_to_existing_string() {
    // (グループ幅, 小数桁数): 3 桁区切り・インド式は（features が有効なら）速い経路、(4, 4) と小数 11 桁は遅い経路
    let cases: [(Grouping, u8); 4] = [
        (3.into(), 2),
        ((3, 2).into(), 0),
        (4.into(), 2),
        (3.into(), 11),
    ];
    let vals: [i64; 8] = [
        0,
        5,
        -5,
        1234567,
        -1234567,
        i32::MAX as i64,
        i32::MIN as i64,
        i64::MIN,
    ];
    for (g, d) in cases {
        let num = NumberFormat::new(NumberParams {
            decimal: ".".into(),
            group: "\u{202f}".into(),
            grouping: g,
            min_grouping: 1,
            frac_digits: d,
            minus_sign: "−".into(),
        })
        .unwrap();
        let cur = CurrencyFormat::new(CurrencyParams {
            decimal: ",".into(),
            group: ".".into(),
            grouping: g,
            min_grouping: 1,
            frac_digits: d,
            positive_prefix: "￥".into(),
            positive_suffix: " 円".into(),
            negative_prefix: "−￥".into(),
            negative_suffix: " 円".into(),
        })
        .unwrap();

        // 書き方ごとに: (中身のある String に追記する関数, 期待する文字列)
        type Writer<'f> = Box<dyn Fn(&mut String, i64) -> Option<String> + 'f>;
        let writers: [(&str, Writer); 4] = [
            (
                "number i64",
                Box::new(|s, v| {
                    num.write_i64(s, v);
                    Some(num.format_i64(v))
                }),
            ),
            (
                "number i32",
                Box::new(|s, v| {
                    let v = i32::try_from(v).ok()?;
                    num.write_i32(s, v);
                    Some(num.format_i32(v))
                }),
            ),
            (
                "currency i64",
                Box::new(|s, v| {
                    cur.write_i64(s, v);
                    Some(cur.format_i64(v))
                }),
            ),
            (
                "currency i32",
                Box::new(|s, v| {
                    let v = i32::try_from(v).ok()?;
                    cur.write_i32(s, v);
                    Some(cur.format_i32(v))
                }),
            ),
        ];

        for (name, write) in &writers {
            let ctx = format!("{name} {g:?} d={d}");
            // 1 回ずつ、ASCII とマルチバイト文字で終わる中身の後ろに書く
            for head in ["前", "￥", "abc"] {
                for &v in &vals {
                    let mut s = String::from(head);
                    if let Some(want) = write(&mut s, v) {
                        assert_eq!(s, format!("{head}{want}"), "{ctx} head={head} {v}");
                    }
                }
            }
            // 同じ String に続けて書くと、全部がつながる
            let mut s = String::from("前");
            let mut want = String::from("前");
            for &v in &vals {
                if let Some(w) = write(&mut s, v) {
                    want.push_str(&w);
                    want.push('|');
                    s.push('|');
                }
            }
            assert_eq!(s, want, "{ctx} 連続");
            // 容量いっぱいの String（書くときに再確保が起きる）
            for &v in &vals {
                let mut s = String::with_capacity(16);
                while s.len() < s.capacity() {
                    s.push('x');
                }
                let head = s.clone();
                if let Some(want) = write(&mut s, v) {
                    assert_eq!(s, format!("{head}{want}"), "{ctx} 容量いっぱい {v}");
                }
            }
        }
    }
}

/// tools/gen.py --rust が出力する形の定数（Cow::Borrowed と Grouping::new）が書けて、そのまま使えること
#[test]
fn const_params_like_gen_py_output() {
    use std::borrow::Cow;
    const EN_IN_NUMBER: NumberParams<'static> = NumberParams {
        decimal: Cow::Borrowed("."),
        group: Cow::Borrowed(","),
        grouping: Grouping::new(3, 2),
        min_grouping: 1,
        minus_sign: Cow::Borrowed("-"),
        frac_digits: 0,
    };
    const DE_EUR: CurrencyParams<'static> = CurrencyParams {
        decimal: Cow::Borrowed(","),
        group: Cow::Borrowed("."),
        grouping: Grouping::new(3, 3),
        min_grouping: 1,
        frac_digits: 2,
        positive_prefix: Cow::Borrowed(""),
        positive_suffix: Cow::Borrowed("\u{a0}€"),
        negative_prefix: Cow::Borrowed("-"),
        negative_suffix: Cow::Borrowed("\u{a0}€"),
    };
    let n = NumberFormat::new(NumberParams {
        frac_digits: 2,
        ..EN_IN_NUMBER
    })
    .unwrap();
    assert_eq!(n.format_i64(-123456789), "-12,34,567.89");
    let c = CurrencyFormat::new(DE_EUR).unwrap();
    assert_eq!(c.format_i64(-123456), "-1.234,56\u{a0}€");
}
