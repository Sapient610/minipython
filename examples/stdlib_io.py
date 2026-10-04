"""标准库与文件 IO：math / random（固定种子）/ string / 文本文件读写。"""

import math
import random
import string

print("=== math ===")
print("pi =", round(math.pi, 6), "e =", round(math.e, 6))
print("sqrt(2) =", round(math.sqrt(2), 6))
print("gcd(1071, 462) =", math.gcd(1071, 462), "factorial(10) =", math.factorial(10))
print("log2(1024) =", math.log(1024, 2), "hypot(3, 4) =", math.hypot(3, 4))
print("floor/ceil/trunc:", math.floor(-1.5), math.ceil(-1.5), math.trunc(-1.5))
print("三角:", round(math.sin(0), 6), round(math.cos(0), 6), round(math.degrees(math.pi), 6))
print("判定:", math.isnan(float("nan")), math.isinf(float("inf")), math.isfinite(1.0))

print()
print("=== random（固定种子保证可复现）===")
random.seed(20240607)
values = [random.random() for _ in range(200)]
print("样本数:", len(values), "全部在 [0, 1):", all(0.0 <= v < 1.0 for v in values))
print("均值接近 0.5:", 0.4 < sum(values) / len(values) < 0.6)
print("randint 在范围内:", all(1 <= random.randint(1, 6) <= 6 for _ in range(50)))
print("randrange 步长正确:", all(random.randrange(0, 100, 10) % 10 == 0 for _ in range(50)))
u = random.uniform(-1.0, 1.0)
print("uniform 在范围内:", -1.0 <= u <= 1.0)
print("choice 属于候选集:", random.choice("abcdef") in "abcdef")
deck = list(range(1, 11))
random.shuffle(deck)
print("shuffle 是排列:", sorted(deck) == list(range(1, 11)))
random.seed(1)
first = random.random()
random.seed(1)
print("同种子可复现:", first == random.random())

print()
print("=== string ===")
print("小写字母:", string.ascii_lowercase)
print("数字:", string.digits, "标点前 6 个:", string.punctuation[:6])
random.seed(7)
password = "".join(random.choice(string.ascii_letters + string.digits) for _ in range(12))
print("密码长度:", len(password))
print("字符集正确:", all(c in string.ascii_letters + string.digits for c in password))

print()
print("=== 文件读写 ===")
path = "examples/_sample_output.txt"
with open(path, "w") as f:
    f.write("第一行\n")
    f.writelines(["第二行\n", "第三行\n"])

with open(path) as f:
    lines = f.readlines()
print("读回行数:", len(lines))
for i, line in enumerate(lines, 1):
    print(i, repr(line))

with open(path) as f:
    total = 0
    for line in f:
        total += len(line.strip())
print("总字符数:", total)

with open(path, "a") as f:
    f.write("第四行（追加）\n")

with open(path) as f:
    print("追加后行数:", len(f.read().split("\n")) - 1)

