//! 自分で値を決めた書式（速い経路に当てはまらない組み合わせを含む）を、素朴な参照実装と比べる
use pprint_currency::{CurrencyFormat, CurrencyParams, FormatError, NumberFormat, NumberParams};

/// 参照実装: 10 進文字列を作り、右から 1 文字ずつ数えて区切りを入れる
fn reference(p: &NumberParams, v: i64) -> String {
    let d = p.frac_digits as usize;
    let mut s = v.unsigned_abs().to_string();
    while s.len() < d + 1 {
        s.insert(0, '0');
    }
    let (int, frac) = s.split_at(s.len() - d);
    let (pr, sc) = (
        p.primary_grouping as usize,
        if p.secondary_grouping == 0 {
            p.primary_grouping
        } else {
            p.secondary_grouping
        } as usize,
    );
    let mut body = String::new();
    let grouping = pr > 0 && int.len() >= pr + p.min_grouping as usize;
    let digits: Vec<char> = int.chars().collect();
    for (i, c) in digits.iter().enumerate() {
        let from_right = digits.len() - i; // この桁を含めて右に残る桁数
        if grouping && i > 0 && from_right >= pr && (from_right - pr).is_multiple_of(sc) {
            body.push_str(p.group);
        }
        body.push(*c);
    }
    if d > 0 {
        body.push_str(p.decimal);
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
    for (pr, sc) in [
        (0, 0),
        (3, 3),
        (3, 2),
        (2, 2),
        (2, 3),
        (3, 0),
        (2, 0),
        (4, 4),
        (1, 1),
        (3, 4),
        (4, 3),
    ] {
        for mg in [1u8, 2, 3, 4] {
            for d in [0u8, 1, 2, 3, 4, 5, 8, 9, 10, 15, 19] {
                for (group, decimal) in [(",", "."), ("\u{202f}", ","), ("", "")] {
                    let p = NumberParams {
                        decimal,
                        group,
                        primary_grouping: pr,
                        secondary_grouping: sc,
                        min_grouping: mg,
                        frac_digits: d,
                        positive_prefix: "+[",
                        positive_suffix: "]",
                        negative_prefix: "−(",
                        negative_suffix: ") 円",
                    };
                    let f = NumberFormat::new(p).unwrap();
                    let c = CurrencyFormat::new(CurrencyParams {
                        decimal: p.decimal,
                        group: p.group,
                        primary_grouping: pr,
                        secondary_grouping: sc,
                        min_grouping: mg,
                        frac_digits: d,
                        positive_prefix: p.positive_prefix,
                        positive_suffix: p.positive_suffix,
                        negative_prefix: p.negative_prefix,
                        negative_suffix: p.negative_suffix,
                    })
                    .unwrap();
                    for &v in &vals {
                        let want = reference(&p, v);
                        let mut s = String::from("前");
                        f.write_i64(&mut s, v);
                        assert_eq!(&s["前".len()..], want, "{pr},{sc} mg={mg} d={d} {v}");
                        assert_eq!(
                            c.format_i64(v),
                            want,
                            "currency {pr},{sc} mg={mg} d={d} {v}"
                        );
                        if let Ok(v32) = i32::try_from(v) {
                            assert_eq!(f.format_i32(v32), want, "i32 {pr},{sc} mg={mg} d={d} {v}");
                            assert_eq!(
                                c.format_i32(v32),
                                want,
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
        decimal: ".",
        group: ",",
        primary_grouping: 3,
        secondary_grouping: 3,
        min_grouping: 1,
        frac_digits: 2,
        positive_prefix: "",
        positive_suffix: "",
        negative_prefix: "-",
        negative_suffix: "",
    };
    assert_eq!(
        NumberFormat::new(NumberParams {
            frac_digits: 20,
            ..base
        })
        .unwrap_err(),
        FormatError::FracDigitsTooLarge
    );
    assert_eq!(
        NumberFormat::new(NumberParams {
            min_grouping: 0,
            ..base
        })
        .unwrap_err(),
        FormatError::MinGroupingZero
    );
    assert_eq!(
        NumberFormat::new(NumberParams {
            primary_grouping: 20,
            ..base
        })
        .unwrap_err(),
        FormatError::GroupingTooLarge
    );
}
