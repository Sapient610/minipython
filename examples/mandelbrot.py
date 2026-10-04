"""用 ASCII 字符绘制曼德博集合，纯计算密集型示例。"""

WIDTH = 78
HEIGHT = 24
CHARS = " .:-=+*#%@"


def mandelbrot(cx, cy, max_iter):
    x = 0.0
    y = 0.0
    i = 0
    while i < max_iter:
        x2 = x * x
        y2 = y * y
        if x2 + y2 > 4.0:
            break
        y = 2.0 * x * y + cy
        x = x2 - y2 + cx
        i += 1
    return i


for row in range(HEIGHT):
    cy = -1.25 + 2.5 * row / (HEIGHT - 1)
    line = []
    for col in range(WIDTH):
        cx = -2.2 + 3.0 * col / (WIDTH - 1)
        it = mandelbrot(cx, cy, 40)
        if it >= 40:
            line.append(CHARS[-1])
        else:
            line.append(CHARS[it * len(CHARS) // 40])
    print("".join(line))
