"""较新 / 进阶的 Python 特性：海象运算符、f-string 调试语法、属性钩子、
活动 __dict__、字典合并、动态建类、旧式序列协议与 math 扩充。"""

import math

print("=== 海象运算符 := ===")
n = 0
while (n := n + 3) < 12:
    print("  n =", n)
print("循环结束后:", n)

if (m := 6 * 7) > 40:
    print("m =", m)
values = [1, 2, 3, 4]
total = 0
print("推导式内赋值:", [total := total + v for v in values], "| 外层可见:", total)
print("表达式中复用:", (k := 5) + k * 2)

print()
print("=== f-string 调试语法 ===")
name = "小明"
score = 91.5
print(f"{name=}")
print(f"{name = }")
print(f"{score=:.2f}")
print(f"{name=!r} {name=!s}")
data = {"items": [1, 2], "ok": True}
print(f"{data=}")
print(f"{score / 10 = }")

print()
print("=== 属性钩子 __getattr__ / __setattr__ / __delattr__ ===")


class Logged:
    def __init__(self):
        object.__setattr__(self, "store", {})
        object.__setattr__(self, "trail", [])

    def __setattr__(self, name, value):
        self.trail.append("set " + name)
        self.store[name] = value

    def __getattr__(self, name):
        self.trail.append("get " + name)
        return "默认值:" + name

    def __delattr__(self, name):
        self.trail.append("del " + name)
        del self.store[name]


log = Logged()
log.alpha = 1
log.beta = 2
print("读取:", log.alpha, log.beta)
print("缺失属性:", log.missing)
del log.beta
print("删除后:", log.store)
print("调用轨迹:", log.trail)


class Typed:
    """用 __getattribute__ 在读取时做转换。"""

    def __init__(self, raw):
        object.__setattr__(self, "raw", raw)

    def __getattribute__(self, name):
        if name == "value":
            return int(object.__getattribute__(self, "raw"))
        return object.__getattribute__(self, name)


t = Typed("42")
print("转换后的值:", t.value, "| 原始值:", t.raw)


class Fallback:
    __slots__ = ()

    def __getattr__(self, name):
        return lambda *args: "%s(%s)" % (name, ", ".join(str(a) for a in args))


f = Fallback()
print("动态方法:", f.greet("hi"), f.add(1, 2))


class Base:
    def __getattr__(self, name):
        return "base:" + name


class Child(Base):
    pass


print("继承的钩子:", Child().whatever)

print()
print("=== 活动的 __dict__ ===")


class Bag:
    pass


bag = Bag()
bag.x = 1
bag.__dict__["y"] = 2
print("属性:", bag.x, bag.y)
print("视图内容:", sorted(bag.__dict__.items()))
print("与 vars() 一致:", vars(bag) == bag.__dict__)
bag.__dict__.update({"z": 3})
print("update 之后:", sorted(bag.__dict__.keys()), "| z =", bag.z)
del bag.__dict__["x"]
print("删除后:", sorted(bag.__dict__), "| 含 y:", "y" in bag.__dict__, "| 长度:", len(bag.__dict__))

print()
print("=== 字典合并运算符 | 与 |= ===")
defaults = {"color": "red", "size": 10}
custom = {"size": 20, "weight": 5}
merged = defaults | custom
print("合并:", sorted(merged.items()))
print("原字典未被修改:", sorted(defaults.items()))
defaults |= custom
print("原地合并:", sorted(defaults.items()))

print()
print("=== 动态建类 type(name, bases, namespace) ===")
Point = type("Point", (), {"dim": 2, "describe": lambda self: "点(%d, %d)" % (self.x, self.y)})
p = Point()
p.x, p.y = 3, 4
print(Point.__name__, p.dim, p.describe(), isinstance(p, Point))
Shape = type("Shape", (), {"kind": "shape"})
Square = type("Square", (Shape,), {"sides": 4})
sq = Square()
print(Square.__name__, sq.sides, sq.kind, isinstance(sq, Shape), [c.__name__ for c in Square.__mro__])

print()
print("=== 旧式序列协议（只有 __getitem__ 也能迭代）===")


class Squares:
    def __init__(self, count):
        self.count = count

    def __getitem__(self, index):
        if index >= self.count:
            raise IndexError(index)
        return index * index


print("list:", list(Squares(5)))
print("推导式:", [v for v in Squares(6) if v % 2 == 0])
print("tuple/sum:", tuple(Squares(3)), sum(Squares(4)))
print("成员判断:", 9 in Squares(4), 5 in Squares(4))

print()
print("=== math 扩充 ===")
print("prod:", math.prod([1, 2, 3, 4]), math.prod([2, 3], start=10), math.prod([]))
print("isqrt:", math.isqrt(15), math.isqrt(16), math.isqrt(10 ** 12))
print("comb/perm:", math.comb(52, 5), math.perm(5, 3), math.comb(3, 5))
print("lcm:", math.lcm(4, 6), math.lcm(3, 4, 5))
print("dist:", math.dist([0, 0], [3, 4]))
print("modf/frexp/ldexp:", math.modf(3.75), math.frexp(8.0), math.ldexp(0.5, 4))
print("isclose:", math.isclose(1.0, 1.0 + 1e-12), math.isclose(1.0, 1.1))
print("remainder:", math.remainder(5, 3), math.remainder(5.5, 2))
print("cbrt/expm1/log1p:", round(math.cbrt(27), 12), round(math.expm1(1.0), 12), round(math.log1p(1.0), 12))
print("nextafter/ulp:", math.nextafter(1.0, 2.0), math.ulp(1.0))

print()
print("=== zip(strict=True) 与 int.bit_count() ===")
print("长度一致:", list(zip([1, 2], "ab", strict=True)))
try:
    list(zip([1, 2, 3], "ab", strict=True))
except ValueError as e:
    print("ValueError:", e)
print("bit_count:", (255).bit_count(), (0).bit_count(), (-7).bit_count(), (10 ** 6).bit_count())
