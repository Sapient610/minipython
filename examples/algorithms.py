"""算法示例：排序、二分查找、埃氏筛、动态规划、字符串处理。"""


def bubble_sort(items):
    data = list(items)
    n = len(data)
    for i in range(n):
        swapped = False
        for j in range(n - 1 - i):
            if data[j] > data[j + 1]:
                data[j], data[j + 1] = data[j + 1], data[j]
                swapped = True
        if not swapped:
            break
    return data


def quick_sort(items):
    if len(items) <= 1:
        return list(items)
    pivot = items[len(items) // 2]
    left = [x for x in items if x < pivot]
    mid = [x for x in items if x == pivot]
    right = [x for x in items if x > pivot]
    return quick_sort(left) + mid + quick_sort(right)


def binary_search(sorted_items, target):
    lo, hi = 0, len(sorted_items) - 1
    while lo <= hi:
        mid = (lo + hi) // 2
        if sorted_items[mid] == target:
            return mid
        if sorted_items[mid] < target:
            lo = mid + 1
        else:
            hi = mid - 1
    return -1


def sieve(limit):
    flags = [True] * (limit + 1)
    flags[0] = False
    flags[1] = False
    p = 2
    while p * p <= limit:
        if flags[p]:
            for multiple in range(p * p, limit + 1, p):
                flags[multiple] = False
        p += 1
    return [i for i in range(limit + 1) if flags[i]]


def longest_common_subsequence(a, b):
    table = [[0] * (len(b) + 1) for _ in range(len(a) + 1)]
    for i in range(1, len(a) + 1):
        for j in range(1, len(b) + 1):
            if a[i - 1] == b[j - 1]:
                table[i][j] = table[i - 1][j - 1] + 1
            else:
                table[i][j] = max(table[i - 1][j], table[i][j - 1])
    return table[len(a)][len(b)]


data = [5, 2, 9, 1, 5, 6, -3, 8, 0]
print("冒泡排序:", bubble_sort(data))
print("快速排序:", quick_sort(data))
print("内置排序:", sorted(data), sorted(data, reverse=True))

ordered = sorted(data)
print("二分查找 6 →", binary_search(ordered, 6), "| 查找 100 →", binary_search(ordered, 100))

primes = sieve(60)
print("60 以内素数:", primes)
print("个数:", len(primes), "最大的三个:", primes[-3:])

print("LCS(ABCBDAB, BDCABA) =", longest_common_subsequence("ABCBDAB", "BDCABA"))

words = ["apple", "banana", "cherry", "date"]
index = {w: i for i, w in enumerate(words)}
print("单词索引:", sorted(index.items()))
