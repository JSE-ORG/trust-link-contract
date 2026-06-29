import re
from pathlib import Path

ROOT = Path("contracts/escrow")
TARGETS = list(ROOT.glob("src/**/*.rs")) + list(ROOT.glob("tests/**/*.rs"))
CALL_RE = re.compile(r'\.(try_create_escrow|create_escrow)\s*\(')

def find_matching_paren(s, start):
    depth = 0
    i = start
    while i < len(s):
        if s[i] == '(':
            depth += 1
        elif s[i] == ')':
            depth -= 1
            if depth == 0:
                return i
        i += 1
    return -1

def split_args(arg_str):
    args, depth, current = [], 0, []
    for ch in arg_str:
        if ch in '([{':
            depth += 1
        elif ch in ')]}':
            depth -= 1
        if ch == ',' and depth == 0:
            args.append(''.join(current))
            current = []
        else:
            current.append(ch)
    tail = ''.join(current).strip()
    if tail:
        args.append(''.join(current))
    return [a.strip() for a in args if a.strip()]

def detect_env_ref(text, pos):
    window = text[max(0, pos - 2000):pos]
    return "fx.env" if re.search(r'\bfx\.env\b', window) else "env"

def fix_file(path):
    text = path.read_text()
    out, last_end, changed = [], 0, False
    for m in CALL_RE.finditer(text):
        open_paren = m.end() - 1
        close_paren = find_matching_paren(text, open_paren)
        if close_paren == -1:
            continue
        args = split_args(text[open_paren + 1:close_paren])
        if len(args) != 7:
            continue  # already fixed or unexpected shape - leave alone
        env_ref = detect_env_ref(text, m.start())
        var_match = re.match(r'&\s*([\w\.]+)', args[0].strip())
        if not var_match:
            print(f"SKIP (couldn't parse first arg): {path}:{m.start()} -> {args[0]}")
            continue
        var_name = var_match.group(1)
        new_args = (
            [f"&single_payee(&{env_ref}, &{var_name})"]
            + [a.strip() for a in args[1:6]]
            + ["&0_u32"]
            + [args[6].strip()]
        )
        out.append(text[last_end:open_paren + 1])
        out.append("\n        " + ",\n        ".join(new_args) + ",\n    ")
        last_end = close_paren
        changed = True
    out.append(text[last_end:])
    if changed:
        path.write_text(''.join(out))
        print(f"FIXED: {path}")

for f in TARGETS:
    fix_file(f)