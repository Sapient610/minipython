"""函数式风格：闭包、高阶函数、lambda、推导式与排序。"""


def make_counter(start=0, step=1):
    """返回一个带状态的计数器闭包。"""
    state = [start]

    def counter():
        state[0] += step
        return state[0]

    return counter


c1 = make_counter()
c2 = make_counter(100, 10)
print("计数器:", c1(), c1(), c1(), c2(), c2())


def compose(f, g):
    return lambda x: f(g(x))


double = lambda x: x * 2
inc = lambda x: x + 1
print("复合函数:", compose(double, inc)(5), compose(inc, double)(5))

nums = [5, 3, 9, 1, 7, 3]
print("原列表:", nums)
print("升序:", sorted(nums))
print("降序:", sorted(nums, reverse=True))
print("去重:", sorted(set(nums)))
print("平方（map）:", list(map(lambda x: x * x, nums)))
print("筛选（filter）:", list(filter(lambda x: x % 2 == 1, nums)))
print("折叠（sum）:", sum(nums), "最大值:", max(nums), "最小值:", min(nums))
print("累计索引:", list(enumerate(nums[:4])))
print("配对:", list(zip("abcd", nums)))

people = [
    {"name": "Tom", "age": 30},
    {"name": "Ann", "age": 25},
    {"name": "Bob", "age": 35},
]
by_age = sorted(people, key=lambda p: p["age"])
print("按年龄:", [(p["name"], p["age"]) for p in by_age])
print("名字:", [p["name"] for p in people if p["age"] > 26])
print("年龄总和:", sum(p["age"] for p in people))

table = {n: n ** 2 for n in range(1, 6)}
print("平方表:", sorted(table.items()))
print("偶数平方:", {n: n ** 2 for n in range(10) if n % 2 == 0})

matrix = [[1, 2, 3], [4, 5, 6], [7, 8, 9]]
print("转置:", [[row[i] for row in matrix] for i in range(3)])
print("对角:", [matrix[i][i] for i in range(3)])
print("全部元素:", [x for row in matrix for x in row])
print("展平求和:", sum(x for row in matrix for x in row))

words = ["banana", "apple", "cherry"]
print("按长度排序:", sorted(words, key=len), "最长:", max(words, key=len))
print("首字母:", "".join(w[0] for w in words))
print("任意/全部:", any(w.startswith("a") for w in words), all(len(w) > 4 for w in words))
