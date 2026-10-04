"""面向对象：继承、super()、魔术方法、property、类方法。"""


class Shape:
    """所有图形的基类。"""

    count = 0

    def __init__(self, name):
        self.name = name
        Shape.count += 1

    def area(self):
        raise NotImplementedError("子类必须实现 area()")

    def describe(self):
        return "%s 的面积是 %.2f" % (self.name, self.area())

    def __str__(self):
        return "<%s>" % self.name

    def __repr__(self):
        return "Shape(%r)" % self.name


class Rectangle(Shape):
    def __init__(self, w, h):
        super().__init__("矩形")
        self.w = w
        self.h = h

    def area(self):
        return self.w * self.h

    @property
    def is_square(self):
        return self.w == self.h

    @staticmethod
    def unit():
        return Rectangle(1, 1)

    @classmethod
    def square(cls, side):
        return cls(side, side)


class Circle(Shape):
    def __init__(self, r):
        super().__init__("圆")
        self.r = r

    def area(self):
        return 3.141592653589793 * self.r ** 2


class Vector:
    def __init__(self, x, y):
        self.x = x
        self.y = y

    def __add__(self, other):
        return Vector(self.x + other.x, self.y + other.y)

    def __mul__(self, k):
        return Vector(self.x * k, self.y * k)

    def __eq__(self, other):
        return self.x == other.x and self.y == other.y

    def __len__(self):
        return 2

    def __getitem__(self, i):
        return self.x if i == 0 else self.y

    def __repr__(self):
        return "Vector(%s, %s)" % (self.x, self.y)


shapes = [Rectangle(3, 4), Rectangle(5, 5), Circle(1)]
for s in shapes:
    print(s.describe(), "|", s, "| 边长个数:", len(s) if isinstance(s, Vector) else "-")

print("图形总数:", Shape.count)
sq = Rectangle.square(2)
print("正方形:", sq.area(), sq.is_square, Rectangle.unit().area())
print("类型检查:", isinstance(sq, Rectangle), isinstance(sq, Shape), issubclass(Rectangle, Shape))
print("MRO:", [c.__name__ for c in Rectangle.__mro__])

v = Vector(1, 2) + Vector(3, 4)
print("向量:", v, v * 2, v == Vector(4, 6), v[0], v[1], len(v))
print("基类属性:", Shape.count, Rectangle.count)
