"""斐波那契：递归、记忆化、迭代与生成式写法。"""


def fib_recursive(n):
    if n < 2:
        return n
    return fib_recursive(n - 1) + fib_recursive(n - 2)


memo = {}


def fib_memo(n):
    if n in memo:
        return memo[n]
    if n < 2:
        result = n
    else:
        result = fib_memo(n - 1) + fib_memo(n - 2)
    memo[n] = result
    return result


def fib_iter(n):
    a, b = 0, 1
    for _ in range(n):
        a, b = b, a + b
    return a


print("递归:", [fib_recursive(i) for i in range(10)])
print("记忆化:", [fib_memo(i) for i in range(15)])
print("迭代:", [fib_iter(i) for i in range(15)])
print("前 20 项:", [fib_iter(i) for i in range(20)])
print("第 50 项:", fib_iter(50))

# 列表推导式的另一种写法
evens = [x for x in range(20) if x % 2 == 0]
print("偶数:", evens)
print("平方:", [x * x for x in range(1, 8)])
