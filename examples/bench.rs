//! 速度の目安: cargo run --release --example bench
use pprint_currency::{CurrencyFormat, CurrencyParams, NumberFormat, NumberParams};
use std::hint::black_box;
use std::time::Instant;

fn bench(name: &str, f: &dyn Fn(&mut String, usize)) {
    let mut best = f64::MAX;
    for _ in 0..9 {
        let mut buf = String::with_capacity(128);
        let t = Instant::now();
        for i in 0..1_500_000 { buf.clear(); f(&mut buf, i); black_box(&buf); }
        best = best.min(t.elapsed().as_nanos() as f64 / 1_500_000.0);
    }
    println!("   {name:36} {best:5.1} ns");
}

fn cur(p: (u8, u8), d: u8, pre: &'static str, npre: &'static str) -> CurrencyFormat<'static> {
    CurrencyFormat::new(CurrencyParams { decimal: ".", group: ",", primary_grouping: p.0, secondary_grouping: p.1, min_grouping: 1,
        frac_digits: d, positive_prefix: pre, positive_suffix: "", negative_prefix: npre, negative_suffix: "" }).unwrap()
}

fn main() {
    let big: Vec<i64> = (0..1000).map(|i| (i * 7919 - 3_000_000) * 12345).collect();
    let small: Vec<i64> = (0..1000).map(|i| i * 37 - 18000).collect();
    let small32: Vec<i32> = small.iter().map(|&v| v as i32).collect();
    let mid32: Vec<i32> = (0..1000).map(|i| (i * 7919 - 3_000_000) * 700).collect();
    for (label, g) in [("3桁区切り", (3u8, 3u8)), ("インド式 3,2", (3, 2))] {
        println!("-- {label}");
        // 小数 0・2 桁と記号の長さが違う通貨を混ぜる（すべて速い経路）
        let curs = [cur(g, 0, "￥", "-￥"), cur(g, 2, "$", "-$"), cur(g, 2, "元\u{a0}", "-元\u{a0}"), cur(g, 0, "₩", "-₩"), cur(g, 2, "€", "-€")];
        let num2 = NumberFormat::new(NumberParams { decimal: ".", group: ",", primary_grouping: g.0, secondary_grouping: g.1, min_grouping: 1,
            frac_digits: 2, positive_prefix: "", positive_suffix: "", negative_prefix: "-", negative_suffix: "" }).unwrap();
        let num8 = NumberFormat::new(NumberParams { frac_digits: 8, ..NumberParams { decimal: ".", group: ",", primary_grouping: g.0,
            secondary_grouping: g.1, min_grouping: 1, frac_digits: 0, positive_prefix: "", positive_suffix: "", negative_prefix: "-", negative_suffix: "" } }).unwrap();
        bench("Currency write_i64 9〜11桁", &|b, i| curs[i % 5].write_i64(b, black_box(big[i % 1000])));
        bench("Currency write_i64 〜5桁", &|b, i| curs[i % 5].write_i64(b, black_box(small[i % 1000])));
        bench("Currency write_i32 9〜10桁", &|b, i| curs[i % 5].write_i32(b, black_box(mid32[i % 1000])));
        bench("Currency write_i32 〜5桁", &|b, i| curs[i % 5].write_i32(b, black_box(small32[i % 1000])));
        bench("Number(小数2桁) write_i64 9〜11桁", &|b, i| num2.write_i64(b, black_box(big[i % 1000])));
        bench("Number(小数2桁) write_i32 〜5桁", &|b, i| num2.write_i32(b, black_box(small32[i % 1000])));
        bench("Number(小数8桁) write_i64 9〜11桁", &|b, i| num8.write_i64(b, black_box(big[i % 1000])));
        let kwd = cur(g, 3, "KWD\u{a0}", "-KWD\u{a0}");
        bench("Currency 小数3桁 9〜11桁", &|b, i| kwd.write_i64(b, black_box(big[i % 1000])));
        bench("Currency 小数3桁 〜5桁", &|b, i| kwd.write_i64(b, black_box(small[i % 1000])));
    }
    println!("-- minimumGroupingDigits 3（通貨、小数2桁）");
    let mg3 = CurrencyFormat::new(CurrencyParams { decimal: ",", group: ".", primary_grouping: 3, secondary_grouping: 3, min_grouping: 3,
        frac_digits: 2, positive_prefix: "", positive_suffix: " €", negative_prefix: "-", negative_suffix: " €" }).unwrap();
    bench("Currency write_i64 〜5桁", &|b, i| mg3.write_i64(b, black_box(small[i % 1000])));
}
