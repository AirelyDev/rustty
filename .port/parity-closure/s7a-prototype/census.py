"""Measure the container/growth-site census for the allocator threading (non-test code)."""
import os, re, glob, subprocess
ROOT = os.path.abspath(os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..', '..'))
V = ROOT + '/crates/rustty-vt/src/'
C = ROOT + '/crates/rustty-vt-c/src/'
FAMILIES = {
    'terminal': ['terminal.rs', 'screen.rs', 'screen_set.rs', 'tabstops.rs', 'string_map.rs', 'hyperlink.rs',
                 'stream_terminal.rs', 'stream.rs', 'stream_continuation.rs', 'dcs.rs', 'apc/*.rs', 'apc/**/*.rs',
                 'kitty/graphics*.rs', 'kitty/clipboard*.rs', 'selection.rs', 'pagelist/*.rs', 'compress/*.rs',
                 'glyph/*.rs'],
    'render': ['render.rs'],
    'search': ['search/*.rs', 'highlight.rs'],
    'osc': ['osc/*.rs', 'osc/**/*.rs'],
}
FIELD = re.compile(r'^\s*(pub(\([^)]*\))?\s+)?[a-z_][a-z0-9_]*\s*:\s*[^=;(]*\b(Vec|VecDeque|HashMap|HashSet|BTreeMap|Box|String|AutoArrayHashMapUnmanaged|ArrayHashMap|CircBuf|MemoryPool|SegmentedList|MultiArrayList)\b[^;(]*,\s*(//.*)?$')
GROW = re.compile(r'\.(push|push_back|push_front|extend_from_slice|extend|insert|resize|reserve|reserve_exact|put|put_no_clobber|get_or_put|append|to_vec|to_owned|clone)\(')
ALLOCP = re.compile(r'\b(alloc|gpa)\b\s*:\s*Allocator|Allocator\b')

def strip_tests(src):
    # cut at the first top-level #[cfg(test)] mod
    m = re.search(r'(?m)^#\[cfg\(test\)\]\s*\n\s*(pub\s+)?mod\s', src)
    return src[:m.start()] if m else src

def code_lines(src):
    out = []
    for l in src.split('\n'):
        t = l.strip()
        if t.startswith('//'):
            continue
        out.append(l)
    return out

tot = {}
for fam, pats in FAMILIES.items():
    files = set()
    for p in pats:
        files.update(glob.glob(V + p, recursive=True))
    files = sorted(f for f in files if not re.search(r'(tests?|bench)\.rs$', f))
    fields = grow = allocs = lines = 0
    per = []
    for f in files:
        src = strip_tests(open(f).read())
        cl = code_lines(src)
        fi = sum(1 for l in cl if FIELD.match(l))
        gr = sum(len(GROW.findall(l)) for l in cl)
        al = sum(len(ALLOCP.findall(l)) for l in cl)
        fields += fi; grow += gr; allocs += al; lines += len(cl)
        per.append((fi, gr, al, f.replace(V, '')))
    tot[fam] = (len(files), lines, fields, grow, allocs)
    print('== %s: files %d, non-test code lines %d, container fields %d, growth/clone call sites %d, Allocator mentions %d'
          % (fam, len(files), lines, fields, grow, allocs))
    for fi, gr, al, f in sorted(per, reverse=True)[:12]:
        print('   fields %3d grow %4d alloc %3d  %s' % (fi, gr, al, f))

# test-side: how many call sites construct the objects (would need an allocator arg if missing)
def count(pat, where):
    r = subprocess.run(['grep', '-rEc', pat] + where, capture_output=True, text=True)
    return sum(int(l.rsplit(':', 1)[1]) for l in r.stdout.split('\n') if l)
print('PageList::init( call sites (all, incl tests):', count(r'PageList::init\(', [V]))
print('Terminal::init( call sites workspace:', count(r'Terminal::init\(', [ROOT + '/crates']))
print('Screen::init( call sites workspace:', count(r'Screen::init\(', [ROOT + '/crates']))
print('RenderState:: / .update( sites vt:', count(r'RenderState::empty\(', [ROOT + '/crates']))
print('impl blocks in family files (for option A generic edits):')
for fam, pats in FAMILIES.items():
    files = set()
    for p in pats:
        files.update(glob.glob(V + p, recursive=True))
    n = 0
    for f in files:
        n += len(re.findall(r'(?m)^\s*impl\b', open(f).read()))
    print('  ', fam, n)
