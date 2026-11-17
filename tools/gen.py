#!/usr/bin/env python3
"""cldr-json (resolved) から、NumberParams / CurrencyParams に渡す値を作る（ライブラリには CLDR のデータを含めない）。
使い方:
  gen.py <cldr-json の root> <locale>...                      → JSON（全通貨。テストや実行時の読み込み用）
  gen.py --rust <cldr-json の root> <locale>:<通貨>,<通貨>... → Rust の定数（必要な通貨だけ埋め込む用）
    例: gen.py --rust cldr ja:JPY,USD en:USD
ロケール固有の前提は置かない。未対応のデータ（ASCII 以外の数字、未対応の構文）に出会ったら例外で止まる。"""
import json, sys, unicodedata, os

rust_mode = sys.argv[1] == '--rust'
args = sys.argv[2:] if rust_mode else sys.argv[1:]
root = args[0]
want = {}  # locale -> 通貨の集合（None なら全通貨）
for a in args[1:]:
    loc, _, curs = a.partition(':')
    want[loc] = set(curs.split(',')) if curs else None
locales = list(want)
def load(p): return json.load(open(os.path.join(root, p), encoding='utf-8'))
fractions = load('currencyData.json')['supplemental']['currencyData']['fractions']
numsys = load('numberingSystems.json')['supplemental']['numberingSystems']

def rs(s):
    """Rust の文字列リテラル。見えない文字（書式制御・空白類）は Rust の Unicode エスケープで書く"""
    out = []
    for ch in s:
        if ch in '"\\': out.append('\\' + ch)
        elif ch == ' ' or (ch.isprintable() and unicodedata.category(ch)[0] not in 'CZ'): out.append(ch)
        else: out.append('\\u{%x}' % ord(ch))
    return '"' + ''.join(out) + '"'

def split_affixes(sub):
    """サブパターンを (prefix, number部, suffix) に分ける。' による引用を処理。"""
    i, n = 0, len(sub)
    def affix(i, stop):
        segs, lit = [], ''
        while i < n and not stop(sub[i]):
            c = sub[i]
            if c == "'":
                j = sub.index("'", i + 1)
                lit += "'" if j == i + 1 else sub[i+1:j]; i = j + 1; continue
            if c == '¤':
                if sub.startswith('¤¤', i): raise ValueError('¤¤ 以上は未対応: ' + sub)
                if lit: segs.append(('Lit', lit)); lit = ''
                segs.append(('Cur',)); i += 1; continue
            if c in '-+':
                if lit: segs.append(('Lit', lit)); lit = ''
                segs.append(('Minus',) if c == '-' else ('Plus',)); i += 1; continue
            if c in '%‰*': raise ValueError('未対応の記号 %r: %s' % (c, sub))
            lit += c; i += 1
        if lit: segs.append(('Lit', lit))
        return segs, i
    pre, i = affix(0, lambda c: c in '#0123456789,.@')
    j = i
    while j < n and sub[j] in '#0123456789,.@': j += 1
    body = sub[i:j]
    suf, k = affix(j, lambda c: False)
    return pre, body, suf

def grouping(body):
    integer = body.split('.')[0]
    if ',' not in integer: return 0, 0
    parts = integer.split(',')
    primary = len(parts[-1])
    secondary = len(parts[-2]) if len(parts) >= 3 else primary
    return primary, secondary

def compile_pattern(p):
    subs = p.split(';')
    pre, body, suf = split_affixes(subs[0])
    if len(subs) > 1:
        npre, nbody, nsuf = split_affixes(subs[1])
    else:  # UTS #35: 暗黙の負パターンは正パターンの前に '-' を付けたもの
        npre, nsuf = [('Minus',)] + pre, suf
    return dict(pre=pre, suf=suf, npre=npre, nsuf=nsuf, grouping=grouping(body))

def seg_rs(s, loc):
    if s[0] == 'Lit': return 'Seg::Lit(%s)' % rs(s[1])
    if s[0] == 'Cur': return 'Seg::Cur'
    if s[0] == 'Minus': return 'Seg::Lit(%s)' % rs(loc['minus'])   # 負号はビルド時に解決
    raise ValueError(s)

def cur_next_to_number(segs, side):
    """¤ が数値に隣接しているか（間にリテラルが無いか）"""
    if not segs: return False
    return segs[-1][0] == 'Cur' if side == 'pre' else segs[0][0] == 'Cur'

def resolve(segs, symbol, minus):
    return ''.join(symbol if x[0] == 'Cur' else (minus if x[0] == 'Minus' else x[1]) for x in segs)

result = {}
for loc in locales:
    n = load(f'{loc}/numbers.json')['main'][loc]['numbers']
    ns = n['defaultNumberingSystem']
    if numsys[ns]['_type'] != 'numeric' or numsys[ns]['_digits'] != '0123456789':
        raise ValueError('ASCII 以外の数字は未対応: %s (%s)' % (loc, ns))
    sym = n['symbols-numberSystem-' + ns]
    minus = sym['minusSign']
    mg = int(n['minimumGroupingDigits'])
    # 数値: decimalFormats の standard パターン
    dp = compile_pattern(n['decimalFormats-numberSystem-' + ns]['standard'])
    if any(x[0] == 'Cur' for k in ('pre', 'suf', 'npre', 'nsuf') for x in dp[k]): raise ValueError('数値パターンに ¤')
    naff = [resolve(dp[k], '', minus) for k in ('pre', 'suf', 'npre', 'nsuf')]
    number = dict(decimal=sym['decimal'], group=sym['group'], primary_grouping=dp['grouping'][0],
                  secondary_grouping=dp['grouping'][1], min_grouping=mg,
                  positive_prefix=naff[0], positive_suffix=naff[1], negative_prefix=naff[2], negative_suffix=naff[3])
    # 通貨: currencyFormats
    cf = n['currencyFormats-numberSystem-' + ns]
    cdec = sym.get('currencyDecimal', sym['decimal']); cgrp = sym.get('currencyGroup', sym['group'])
    std = compile_pattern(cf['standard'])
    alpha = compile_pattern(cf['standard-alphaNextToNumber']) if 'standard-alphaNextToNumber' in cf else None
    if alpha and alpha['grouping'] != std['grouping']: raise ValueError('alphaNextToNumber とグループ幅が違う: ' + loc)
    sp = cf['currencySpacing']
    for side in ('beforeCurrency', 'afterCurrency'):
        if sp[side]['currencyMatch'] != '[[:^S:]&[:^Z:]]' or sp[side]['surroundingMatch'] != '[:digit:]':
            raise ValueError('未対応の currencySpacing: %r' % sp[side])
    curs = load(f'{loc}/currencies.json')['main'][loc]['numbers']['currencies']
    codes = sorted(curs) if want[loc] is None else sorted(want[loc])
    currencies = {}
    for code in codes:
        if code not in curs: raise ValueError('%s に %s のデータがない' % (loc, code))
        if 'pattern' in curs[code]: raise ValueError('通貨固有パターンは未対応: ' + code)
        symb = curs[code].get('symbol', code)
        d = int(fractions.get(code, fractions['DEFAULT'])['_digits'])
        # ¤ が数値に隣接する側の、数値に最も近い記号の文字で、空白を入れるか決める（ICU の出力と一致することを確認済み）
        if cur_next_to_number(std['pre'], 'pre'): ch = symb[-1]
        elif cur_next_to_number(std['suf'], 'suf'): ch = symb[0]
        else: ch = None
        P, sym_out = std, symb
        if ch is not None and unicodedata.category(ch)[0] not in 'SZ':
            if alpha: P = alpha
            elif cur_next_to_number(std['pre'], 'pre'): sym_out = symb + sp['afterCurrency']['insertBetween']
            else: sym_out = sp['beforeCurrency']['insertBetween'] + symb
        aff = [resolve(P[k], sym_out, minus) for k in ('pre', 'suf', 'npre', 'nsuf')]
        currencies[code] = dict(decimal=curs[code].get('decimal', cdec), group=curs[code].get('group', cgrp),
                                primary_grouping=std['grouping'][0], secondary_grouping=std['grouping'][1],
                                min_grouping=mg, frac_digits=d,
                                positive_prefix=aff[0], positive_suffix=aff[1], negative_prefix=aff[2], negative_suffix=aff[3])
    result[loc] = dict(number=number, currencies=currencies)

if not rust_mode:
    print(json.dumps(result, ensure_ascii=False, indent=1))
else:
    def fields(d):
        return ', '.join('%s: %s' % (k, rs(v) if isinstance(v, str) else v) for k, v in d.items())
    out = ['// 自動生成（tools/gen.py --rust）。データ: Unicode CLDR（cldr-json）',
           'use pprint_currency::{CurrencyParams, NumberParams};', '']
    for loc, r in result.items():
        ident = loc.replace('-', '_').upper()
        out.append('/// %s の数値（frac_digits は用途に合わせて上書きする）' % loc)
        out.append('pub const %s_NUMBER: NumberParams<\'static> = NumberParams { %s, frac_digits: 0 };' % (ident, fields(r['number'])))
        for code, c in r['currencies'].items():
            out.append('pub const %s_%s: CurrencyParams<\'static> = CurrencyParams { %s };' % (ident, code, fields(c)))
        out.append('')
    print('\n'.join(out))
