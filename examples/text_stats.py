"""文本处理：字符串方法、切片、格式化与字典统计。"""

TEXT = """the quick brown fox jumps over the lazy dog
the dog barks and the fox runs away
quick quick slow"""

lines = TEXT.split("\n")
print("行数:", len(lines), "字符数:", len(TEXT))

words = TEXT.replace("\n", " ").split()
print("单词数:", len(words))
print("前 5 个:", words[:5])
print("倒序前 5 个:", words[::-1][:5])

counter = {}
for w in words:
    counter[w] = counter.get(w, 0) + 1
print("统计:", sorted(counter.items(), key=lambda kv: (-kv[1], kv[0])))

longest = max(words, key=len)
print("最长单词:", longest, "长度:", len(longest))
print("首字母大写:", " ".join(w.capitalize() for w in words[:6]))
print("去重后按字母序:", sorted(set(words))[:8])

for i, line in enumerate(lines, 1):
    print("%d| %-45s| %d" % (i, line, len(line)))

print()
print("--- 格式化 ---")
pi = 3.141592653589793
print(f"pi = {pi:.4f}, 科学计数 = {pi:.3e}")
print(f"{'名称':<10}{'数量':>6}{'占比':>9}")
for name, n in [("苹果", 12), ("香蕉", 7), ("樱桃", 31)]:
    total = 50
    print(f"{name:<10}{n:>6}{n / total:>9.2%}")
print("填充与对齐: [{:*^11}] [{:>11}] [{:<11}]".format("居中", "右", "左"))
print("十六进制: {:x} {:X} {:#x} 二进制: {:08b}".format(255, 255, 255, 5))
