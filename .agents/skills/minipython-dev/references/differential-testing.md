# 与 CPython 差分测试

本项目的验收标准是**输出逐字节一致**，因此"改完代码跑一个脚本、两边 diff"是最主要的验证手段。
本文件记录完整流程、结果判读与分诊经验。

## 三层验证，各管一段

| 层次 | 位置 | 管什么 |
| --- | --- | --- |
| 断言测试 | `tests/language.rs` | 具体行为的**精确期望值**（含异常类型与消息、traceback 内容） |
| 示例逐字节 | `examples/*.py` + `examples/*.out` | 端到端：10 个程序的所有输出与 CPython 完全一致 |
| 自动差分 | `tests/cpython_diff.rs` | CI/本地有 CPython 时，自动把示例交给 CPython 再与 `.out` 比一次（确认期望值不是"本机特有"） |

三层都过，才算这个特性做完。

## 手工差分流程（开发新特性时用）

```bash
# 1) 写临时脚本（放仓库根或 target/ 下，别提交）
# 2) 分别运行：用已构建的 exe，不要用 cargo run
python -W ignore t.py > t.py.txt 2>&1
./target/debug/minipython.exe t.py > t.mp.txt 2>&1
#    需要看真实 stdout/stderr 顺序时分开重定向
./target/debug/minipython.exe t.py > t.mp.txt 2> t.err.txt

# 3) 用 Python 比对（PowerShell 的 Compare-Object 在中文与换行上不可靠）
python -c "import io;a=io.open('t.py.txt',encoding='utf-8').read().splitlines();b=io.open('t.mp.txt',encoding='utf-8').read().splitlines();d=[(i+1,a[i],b[i]) for i in range(min(len(a),len(b))) if a[i]!=b[i]];print('diffs:',len(d));[print(x) for x in d[:20]]"
```

环境变量（Windows 上必须）：`PYTHONIOENCODING=utf-8`、`PYTHONUTF8=1`，否则 CPython 用 GBK 输出中文，diff 全是"假差异"。

## 生成 `examples/*.out`（新增示例后必须）

```bash
python -c "import io,subprocess,os;e=dict(os.environ,PYTHONIOENCODING='utf-8',PYTHONUTF8='1');r=subprocess.run(['python','-W','ignore','examples/x.py'],capture_output=True,env=e);assert r.returncode==0, r.stderr.decode('utf-8','replace');io.open('examples/x.out','w',encoding='utf-8',newline='\n').write(r.stdout.decode('utf-8').replace('\r\n','\n'))"
```

- 必须 **LF**、UTF-8、无 BOM；`.gitattributes` 已强制 LF，`tests/examples.rs` 也会把 `\r\n` 归一化
- **不要手写** `.out`；`.out` 是 CPython 的输出，手写就等于自己给自己出题
- 示例必须**确定性**：不要用未固定种子的 `random`、不要依赖时间/路径/环境；集合一律 `sorted()` 后再打印

## 结果分诊

**必须修（真 bug）**

- 异常**类型**不同（`TypeError` vs `ValueError`）、有/无异常
- 数值、字符串、容器内容、控制流结果不同
- traceback 缺少帧或源码行（`tests/language.rs` 有专门断言）

**可以容忍但要记录**

- 异常**消息文字**不同（类型一致即可）：优先贴近 CPython，实在不一致就接受，别为此扭曲实现
- `repr` 里的地址（`<Foo object at 0x...>`）——测试里避免打印未定义 `__repr__` 的对象
- `set`/`dict` 的**未排序**打印顺序（本实现是插入序，CPython 是哈希序）→ 测试里 `sorted()`

**平台差异（不改实现，改测试容差）**

- `expm1`/`log1p`/`cbrt`/`erf` 之类调用系统 libm 的函数，在 **macOS(arm64)** 上最后一位可能与 glibc 不同
- 处理方式：`round(value, 12)` 后比较；IEEE 精确运算（`nextafter`/`ulp`/`frexp`/`ldexp`/`modf`/`dist`/`prod`/`isqrt`/`comb`/`perm`/`lcm`/`remainder`）可逐位比较
- **实例**：CI 第一次跑 macOS 就撞上 `math.expm1(1.0)` 末位不同 —— 这正是这条规则的由来

**应当写进"已知限制"而不是修的**

只有当你确认它是**有意的设计取舍**时才这么做（i64 溢出转 float、`set` 迭代序、无 bigint、容器不调用用户 `__hash__`…），并且同步更新 `README.md` 的「已知限制」小节。

## 别被骗的几个坑

1. **`cargo run` 的警告混进 stdout** → 用 `.\target\debug\minipython.exe` 或 `--quiet`
2. **PowerShell 重定向交错 stdout/stderr** → traceback 会莫名出现在文件中间；分开重定向或用 `2>&1 | Out-File`
3. **CRLF**：PowerShell `>` 写出的是 CRLF，用 Python 读写并显式 `newline='\n'`
4. **中文乱码**：忘了 `PYTHONIOENCODING=utf-8`
5. **比较行数不等时**：`splitlines()` 后按 `min(len)` 比会漏掉尾部差异，也要单独比长度
6. **差分脚本自身写错**：先用一个已知正确的脚本（如 `examples/classes.py`）验证差分流程本身能报"0 diffs"
