//! 语言特性测试：覆盖词法、语法、作用域、类、异常、内置函数与模块。

use minipython::{run_capture, run_or_error, Session};

fn out(src: &str) -> String {
    run_capture(src).unwrap_or_else(|e| panic!("执行失败: {}\n源码:\n{}", e, src))
}

/// 返回 (输出, 异常摘要)
fn err(src: &str) -> String {
    let (o, e) = run_or_error(src);
    assert!(e.is_some(), "期望抛出异常，但正常结束，输出: {}", o);
    e.unwrap()
}

#[test]
fn arithmetics_and_numbers() {
    assert_eq!(out("print(7 // 2, -7 // 2, 7 % 3, -7 % 3)"), "3 -4 1 2\n");
    assert_eq!(out("print(2 ** 10, 7 / 2)"), "1024 3.5\n");
    assert_eq!(out("print(0.1 + 0.2)"), "0.30000000000000004\n");
    assert_eq!(out("print(1e16, 1e15, 1.5e-7)"), "1e+16 1000000000000000.0 1.5e-07\n");
    assert_eq!(out("print(int('ff', 16), float('2.5'), abs(-3))"), "255 2.5 3\n");
    assert_eq!(out("print(divmod(-7, 2))"), "(-4, 1)\n");
    assert_eq!(out("print(255, hex(255), oct(8), bin(5))"), "255 0xff 0o10 0b101\n");
}

#[test]
fn strings_and_formatting() {
    assert_eq!(out("print('abc'.upper(), 'ABC'.lower())"), "ABC abc\n");
    assert_eq!(out("print('a,b,c'.split(','), '-'.join('abc'))"), "['a', 'b', 'c'] a-b-c\n");
    assert_eq!(out("print('hello'[1:3], 'hello'[::-1], len('hello'))"), "el olleh 5\n");
    assert_eq!(out("print(f'{3.14159:.2f}|{42:>5}|{255:#x}')"), "3.14|   42|0xff\n");
    assert_eq!(out("print('%s-%d-%.2f' % ('a', 1, 2.5))"), "a-1-2.50\n");
    assert_eq!(out("print('{} {1} {0}'.format('a', 'b'))"), "a b a\n");
    assert_eq!(out("print(repr('a\\nb'), str(1.0))"), "'a\\nb' 1.0\n");
}

#[test]
fn containers() {
    assert_eq!(out("print([1, 2] + [3], [1] * 3, (1, 2) + (3,))"), "[1, 2, 3] [1, 1, 1] (1, 2, 3)\n");
    assert_eq!(
        out("d = {'a': 1}\nd['b'] = 2\nprint(sorted(d.items()), d.get('z', 0))"),
        "[('a', 1), ('b', 2)] 0\n"
    );
    assert_eq!(out("print(sorted({1, 2} | {3}), 2 in [1, 2], 'a' in 'abc')"), "[1, 2, 3] True True\n");
    assert_eq!(out("a = [3, 1, 2]\na.sort()\nprint(a, sorted(a, reverse=True))"), "[1, 2, 3] [3, 2, 1]\n");
    assert_eq!(out("print([x * 2 for x in range(3)], {k: k for k in 'ab'})"), "[0, 2, 4] {'a': 'a', 'b': 'b'}\n");
    assert_eq!(out("x = [1, 2, 3]\ndel x[0]\nprint(x)"), "[2, 3]\n");
}

#[test]
fn control_flow() {
    let src = "
total = 0
for i in range(10):
    if i % 2 == 0:
        continue
    if i > 7:
        break
    total += i
print(total)
n = 0
while n < 3:
    n += 1
else:
    print('else', n)
print('big' if n > 2 else 'small')
";
    assert_eq!(out(src), "16\nelse 3\nbig\n");
}

#[test]
fn functions_closures_and_scope() {
    let src = "
def f(a, b=2, *args, **kw):
    return a + b + sum(args) + len(kw)
print(f(1), f(1, 1), f(1, 1, 2, 3), f(1, k=1))
def outer():
    n = 0
    def inner():
        nonlocal n
        n += 1
        return n
    return inner
c = outer()
print(c(), c())
g = 1
def bump():
    global g
    g += 10
bump()
print(g)
print((lambda x, y=2: x * y)(3))
";
    assert_eq!(out(src), "3 2 7 4\n1 2\n11\n6\n");
}

#[test]
fn classes_and_inheritance() {
    let src = "
class A:
    kind = 'A'
    def __init__(self, v):
        self.v = v
    def show(self):
        return 'A' + str(self.v)
    def __str__(self):
        return 'A(%d)' % self.v
class B(A):
    kind = 'B'
    def show(self):
        return 'B' + super().show()
b = B(3)
print(b.show(), b.kind, A.kind, str(b), isinstance(b, A), issubclass(B, A))
print([c.__name__ for c in B.__mro__])
class P:
    def __init__(self):
        self._x = 0
    @property
    def x(self):
        return self._x
    @x.setter
    def x(self, v):
        self._x = v * 2
p = P()
p.x = 5
print(p.x)
class V:
    def __init__(self, n):
        self.n = n
    def __add__(self, o):
        return V(self.n + o.n)
    def __eq__(self, o):
        return self.n == o.n
    def __repr__(self):
        return 'V(%d)' % self.n
print(V(1) + V(2), V(1) == V(1))
";
    assert_eq!(
        out(src),
        "BA3 B A A(3) True True\n['B', 'A', 'object']\n10\nV(3) True\n"
    );
}

#[test]
fn exceptions() {
    let src = "
class MyError(Exception):
    pass
try:
    raise MyError('boom')
except MyError as e:
    print('caught', e, type(e).__name__)
try:
    1 / 0
except ZeroDivisionError as e:
    print('zero')
finally:
    print('finally')
try:
    raise ValueError('v')
except (TypeError, ValueError) as e:
    print('tuple', e)
def f():
    try:
        return 'try'
    finally:
        print('cleanup')
print(f())
";
    assert_eq!(
        out(src),
        "caught boom MyError\nzero\nfinally\ntuple v\ncleanup\ntry\n"
    );
}

#[test]
fn error_kinds_match_python() {
    assert!(err("1 / 0").contains("ZeroDivisionError"));
    assert!(err("[1][5]").contains("IndexError"));
    assert!(err("{}['k']").contains("KeyError"));
    assert!(err("undefined_name").contains("NameError"));
    assert!(err("int('x')").contains("ValueError"));
    assert!(err("1 + 'a'").contains("TypeError"));
    assert!(err("'a'.nope()").contains("AttributeError"));
    assert!(err("import not_a_real_module").contains("ModuleNotFoundError"));
    assert!(err("[][0]").contains("IndexError"));
    assert!(err("assert False, 'msg'").contains("AssertionError"));
}

#[test]
fn builtin_functions() {
    assert_eq!(
        out("print(len('abc'), sum(range(5)), min([3, 1]), max(3, 1), abs(-2))"),
        "3 10 1 3 2\n"
    );
    assert_eq!(
        out("print(list(zip([1, 2], 'ab')), list(enumerate('ab', 1)), list(reversed([1, 2])))"),
        "[(1, 'a'), (2, 'b')] [(1, 'a'), (2, 'b')] [2, 1]\n"
    );
    assert_eq!(
        out("print(list(map(lambda x: x + 1, [1, 2])), list(filter(lambda x: x > 1, [1, 2, 3])))"),
        "[2, 3] [2, 3]\n"
    );
    assert_eq!(
        out("print(sorted(['bb', 'a'], key=len), all([1, 2]), any([0, 1]), round(2.675, 2))"),
        "['a', 'bb'] True True 2.67\n"
    );
    assert_eq!(
        out("print(isinstance(1, int), issubclass(bool, int), callable(print), callable(1))"),
        "True True True False\n"
    );
    assert_eq!(out("print(chr(65), ord('A'), getattr('a', 'upper')())"), "A 65 A\n");
}

#[test]
fn modules_math_string_sys() {
    assert_eq!(out("import math\nprint(round(math.sqrt(2), 4), math.gcd(12, 18))"), "1.4142 6\n");
    assert_eq!(out("from math import pi\nprint(round(pi, 2))"), "3.14\n");
    assert_eq!(out("import string\nprint(string.digits, string.ascii_lowercase[:3])"), "0123456789 abc\n");
    assert_eq!(out("import sys\nprint(isinstance(sys.argv, list))"), "True\n");
}

#[test]
fn random_is_reproducible_with_seed() {
    let a = out("import random\nrandom.seed(42)\nprint([random.randint(1, 100) for _ in range(4)])");
    let b = out("import random\nrandom.seed(42)\nprint([random.randint(1, 100) for _ in range(4)])");
    assert_eq!(a, b);
}

#[test]
fn file_io_roundtrip() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test_io");
    std::fs::create_dir_all(&dir).expect("创建测试目录失败");
    let path = dir.join("data.txt");
    let path_str = path.to_string_lossy().replace('\\', "/");
    let src = format!(
        "
with open('{p}', 'w') as f:
    f.write('a\\n')
    f.writelines(['b\\n', 'c\\n'])
with open('{p}') as f:
    lines = f.readlines()
print(len(lines), repr(lines[0]))
with open('{p}') as f:
    total = 0
    for line in f:
        total += 1
print(total)
",
        p = path_str
    );
    assert_eq!(out(&src), "3 'a\\n'\n3\n");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn user_module_import() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("target/test_import");
    std::fs::create_dir_all(&dir).expect("创建测试目录失败");
    std::fs::write(
        dir.join("helper_mod.py"),
        "VALUE = 7\n\ndef triple(x):\n    return x * 3\n",
    )
    .unwrap();
    let path = dir.join("main.py");
    std::fs::write(
        &path,
        "import helper_mod\nfrom helper_mod import triple\nprint(helper_mod.VALUE, triple(2))\n",
    )
    .unwrap();
    let mut s = Session::new();
    s.run_file(&path.to_string_lossy()).expect("导入失败");
    assert_eq!(s.output(), "7 6\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn traceback_contains_frames_and_source() {
    let src = "def inner():\n    raise ValueError('deep')\n\ndef outer():\n    inner()\n\nouter()\n";
    let (_, e) = run_or_error(src);
    let t = e.unwrap();
    assert!(t.contains("Traceback (most recent call last):"), "{}", t);
    assert!(t.contains("in inner"), "{}", t);
    assert!(t.contains("in outer"), "{}", t);
    assert!(t.contains("in <module>"), "{}", t);
    assert!(t.contains("raise ValueError('deep')"), "{}", t);
    assert!(t.trim_end().ends_with("ValueError: deep"), "{}", t);
}

#[test]
fn syntax_errors_are_reported_with_line() {
    let (_, e) = run_or_error("x = 1\ndef f(:\n    pass\n");
    let msg = e.unwrap();
    assert!(msg.contains("SyntaxError"), "{}", msg);
    assert!(msg.contains("line 2") || msg.contains("(line 2)"), "{}", msg);
}

#[test]
fn deep_recursion_is_bounded_not_crashing() {
    let src = "def f(n):\n    return f(n + 1)\n\nf(0)\n";
    let (_, e) = run_or_error(src);
    assert!(e.unwrap().contains("RecursionError"));
}

#[test]
fn recursion_works_reasonably_deep() {
    let src = "def fact(n):\n    return 1 if n <= 1 else n * fact(n - 1)\nprint(fact(20))\n";
    assert_eq!(out(src), "2432902008176640000\n");
}

#[test]
fn while_and_for_else_semantics() {
    assert_eq!(
        out("for i in range(3):\n    pass\nelse:\n    print('done')"),
        "done\n"
    );
    assert_eq!(
        out("for i in range(3):\n    break\nelse:\n    print('done')\nprint('after')"),
        "after\n"
    );
}

#[test]
fn star_unpacking_and_kwargs() {
    let src = "
def f(*args, **kw):
    return len(args), sorted(kw.items())
print(f(*[1, 2, 3], **{'a': 1}))
a, *rest = [1, 2, 3]
print(a, rest)
x, y = 1, 2
print(x, y)
";
    assert_eq!(out(src), "(3, [('a', 1)])\n1 [2, 3]\n1 2\n");
}

#[test]
fn context_manager_protocol() {
    let src = "
class CM:
    def __init__(self, name, suppress):
        self.name = name
        self.suppress = suppress
    def __enter__(self):
        print('enter', self.name)
        return self.name
    def __exit__(self, t, v, tb):
        print('exit', self.name, t.__name__ if t else None)
        return self.suppress

with CM('a', False) as x:
    print('body', x)

with CM('b', True):
    raise ValueError('boom')

print('survived')
";
    assert_eq!(
        out(src),
        "enter a\nbody a\nexit a None\nenter b\nexit b ValueError\nsurvived\n"
    );
}

#[test]
fn decorators_work() {
    let src = "
def double(f):
    def wrapper(*args):
        return f(*args) * 2
    return wrapper

@double
def five():
    return 5

def tag(c):
    c.tag = 'ok'
    return c

@tag
class C:
    pass

print(five(), C.tag, type(C).__name__)
";
    assert_eq!(out(src), "10 ok type\n");
}

#[test]
fn unicode_strings() {
    assert_eq!(out("print(len('中文'), '中文'[0], 'héllo'.upper())"), "2 中 HÉLLO\n");
    assert_eq!(out("print(sorted('cba'), '→'.join(['a', 'b']))"), "['a', 'b', 'c'] a→b\n");
}

#[test]
fn nested_containers_repr() {
    assert_eq!(
        out("print({'k': [1, {'n': (2, 3)}]}, (1,), [], set())"),
        "{'k': [1, {'n': (2, 3)}]} (1,) [] set()\n"
    );
}

#[test]
fn self_referencing_list_does_not_hang() {
    let src = "a = [1]\na.append(a)\nprint(a)\n";
    assert_eq!(out(src), "[1, [...]]\n");
}
