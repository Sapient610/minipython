<!-- 标题建议：类型: 中文摘要（feat / fix / test / docs / ci / chore / refactor） -->

## 这个 PR 做了什么

<!-- 一到两句；如果修的是 issue，写 "Closes #123" -->

## 为什么这样做

<!-- 关键设计取舍；若有其它实现方案，说明为什么没选它 -->

## 验证

<!-- 用命令和结果说话，不要只写「测试通过」 -->

- [ ] `cargo fmt --all -- --check` 通过
- [ ] `cargo clippy --all-targets -- -D warnings` 通过
- [ ] `cargo test` 全绿（共 __ 项）

与 CPython 3.13 的对照结论（本项目验收标准）：

<!--
  例如：
  - 新增/改动行为的脚本在两边输出**逐字节一致**（贴 diff 结论：0 diffs）
  - 或：异常类型一致，消息文字有差异（差异点：...）
  - 或：涉及平台 libm 的浮点值，已 round(..., 12) 后比较
-->

## 清单

- [ ] 新增/修改的**行为**有回归用例（优先补在 `tests/language.rs`）
- [ ] 若新增示例：期望输出 `examples/*.out` 由 **CPython 生成**（LF 换行，未手写）
- [ ] 同步了 `README.md`（支持矩阵 / 已知限制 / 特性一览）
- [ ] 同步了 `CHANGELOG.md`
- [ ] 若新增了约定或踩坑点，同步了 `AGENTS.md`
- [ ] **没有**引入第三方 crate / `build.rs` / `unsafe`（本项目硬约束）
- [ ] 没有提交 `target/`、`examples/_sample_output.txt` 等产物

## 备注

<!-- 已知的遗留问题、后续计划、需要 reviewer 特别关注的地方 -->
