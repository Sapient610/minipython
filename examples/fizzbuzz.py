"""FizzBuzz：最经典的控制流练习。"""


def fizzbuzz(n):
    if n % 15 == 0:
        return "FizzBuzz"
    if n % 3 == 0:
        return "Fizz"
    if n % 5 == 0:
        return "Buzz"
    return str(n)


line = []
for i in range(1, 21):
    line.append(fizzbuzz(i))
    if len(line) == 5:
        print(" ".join(line))
        line = []
