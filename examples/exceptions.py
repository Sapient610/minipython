"""异常处理：自定义异常、finally、异常链与断言。"""


class ValidationError(Exception):
    """数据校验失败。"""


class RangeError(ValidationError):
    def __init__(self, value, low, high):
        self.value = value
        self.low = low
        self.high = high
        super().__init__(
            "%s 不在 [%s, %s] 范围内" % (value, low, high)
        )


def check(value, low=0, high=100):
    if not isinstance(value, int):
        raise ValidationError("需要整数，而不是 %s" % type(value).__name__)
    if value < low or value > high:
        raise RangeError(value, low, high)
    return value


for candidate in [50, -3, "abc", 200]:
    try:
        result = check(candidate)
        print("通过:", result)
    except RangeError as e:
        print("范围错误:", e, "| 值 =", e.value)
    except ValidationError as e:
        print("校验错误:", e)
    except Exception as e:
        print("未知错误:", type(e).__name__, e)
    finally:
        print("  -- 处理完毕")

print()
print("=== 常见内置异常 ===")
cases = [
    lambda: 1 / 0,
    lambda: [1, 2, 3][10],
    lambda: {"k": 1}["missing"],
    lambda: int("abc"),
    lambda: undefined_variable,
    lambda: "a" + 1,
]
for i, case in enumerate(cases, 1):
    try:
        case()
    except (ZeroDivisionError, IndexError, KeyError) as e:
        print(i, "索引/算术异常:", type(e).__name__, e)
    except (ValueError, TypeError) as e:
        print(i, "类型/取值异常:", type(e).__name__, e)
    except Exception as e:
        print(i, "其他异常:", type(e).__name__, e)

print()
print("=== 断言与重新抛出 ===")


def divide(a, b):
    try:
        assert b != 0, "除数不能为 0"
        return a / b
    except AssertionError as e:
        print("断言失败:", e)
        raise ValueError("无法完成除法") from None


try:
    divide(10, 0)
except ValueError as e:
    print("外层捕获:", e)

try:
    try:
        raise ValidationError("第一层")
    except ValidationError:
        raise RuntimeError("第二层")
except RuntimeError as e:
    print("链式异常最终类型:", type(e).__name__, e)
