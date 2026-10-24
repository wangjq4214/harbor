# UI 描述宏与 Widget 基础 API 审阅及改进建议

## 1. 结论

建议做一轮 **API v2**，而不只是更换宏语法。

目前的复杂性主要来自构建生命周期、子节点协议、回调模型和布局组合不一致。建议方向是：

> 统一构建语义，保留普通 Rust 表达式；明确组件的子节点契约；以 Flutter 式显式组件树表达布局，不扩张通用链式包装 API。

本文是源码审阅和设计建议。允许讨论破坏性变更。**状态更新（2026-10-23）：F1、F2 已实施并通过各自记录的自动化验证；F3 已实施并通过自动化验证及 native hidden-host smoke，但原生 IME 与实际 HMR DLL 卸载验收仍待完成；F4 已实施并通过自动化验证及 GPU/系统字体渲染验证，原生应用焦点/IME 与实际 HMR generation teardown 仍待验收，见第 9 节；F5–F8 仍为待实施建议。** 第 2–7 节保留原审阅基线与建议，不代表实施后的全部现状；F3 原建议中的 `on_activate` / `enabled` 命名方向已被后续决策修订。F5 及相关目标示例已按后续反馈改为显式声明式组件树，仍未实施。

## 2. 审阅基线与范围

- 源码版本：`99d4bc06d75976153e3fc70b470a777d461091ee`。
- 审阅开始和结束时工作树均干净；本文是在审阅结束后新增的报告文件。
- 模式：分析与建议，不实施重构。
- 覆盖：
  - `crates/harbor-widget-macros/src/` 的 4 个生产源码文件。
  - `crates/harbor-widget/src/widgets/` 的全部 31 个生产源码文件，含模块导出文件。
  - `construction.rs`、`lib.rs` 和 `view.rs` 的公开构建 API。
  - 与构建身份、hooks、主题相关的 reconciliation/runtime 接口。
  - tabs、search、appearance 等应用调用，以及相关宏、child construction 测试和文档。
- 测试采用代表性契约阅读，未完整审计所有测试实现。
- 不纳入全面审计：GPU 渲染器、native host、终端引擎和平台适配内部。
- 未运行构建、测试或原生交互验证；下文收益均为设计预期，不是已经实现的结果。

### 当前行为路径

```text
view! 根节点 ── Component::build(cx) ── concrete View
view! 子节点 ── IntoChildView ── deferred View ── Fiber 内构建
已有 View 子节点 ── 原样传递

父节点 child block ── Children ── WithChildren
                                 └─ 错误由宏转成 panic
```

这个路径说明：表面相似的节点声明，并不总有相同的构建时机和状态归属。

## 3. 主要发现与建议

### F1 / P0：统一根节点、子节点和内容 slot 的构建边界

#### 现状与影响

- 宏根节点直接调用 `Component::build(cx)`。
- 子节点通过 `IntoChildView` 延迟到 reconciliation 时构建。
- 已经构建好的 `View` 可以作为子节点，却不能作为宏根节点。
- `InteractiveRegion::content` 使用所属 region 的 `BuildCx` 执行内容构造函数。

因此，提取 UI 函数或调整嵌套位置时，调用者需要判断 hooks 属于谁，不能把所有 UI 组合都当成等价的树描述。应用的 tab row 甚至使用零 padding 包装，明确隔离 stateful region 的 Fiber。

证据：

- `crates/harbor-widget-macros/src/expand.rs:50–150`
- `crates/harbor-widget/src/construction.rs`：`IntoChildView`
- `crates/harbor-widget/src/widgets/interactive_region.rs:137`
- `crates/harbor-app/src/tab_view_ui.rs`：`tab_row`
- `crates/harbor-widget-macros/tests/ui/fail/view_root.rs`

#### 建议

- `view!` 只构造 UI 描述，不立即执行根组件。
- 根和子节点采用相同的描述转换协议。
- 普通 UI 组合不需要传递 `cx`；只有读取状态、主题等组件逻辑需要它。
- 状态驱动内容 slot 应有明确的组件边界，不能默认借用父组件的 hook 序列。
- 保留显式所有权，不隐式 clone，也不以便利语法掩盖 `'static` 约束。

#### 风险与前置条件

这不是仅修改宏展开就能完成的优化。

当前 `crates/harbor-widget/src/fiber/reconcile.rs:126–130` 对组件返回值调用 `decompose()`，要求它已经是 concrete View；`View::decompose` 遇到 deferred View 会 panic。根和子节点统一延迟构建，需要同时明确组件身份、渲染节点与返回树的关系，不能简单替换为 `View::deferred(...)` 或无边界地循环展开。

需要验证：状态归属、key/type 身份、主题继承、外部资源注册、订阅清理、卸载，以及 HMR 旧 generation 回调释放顺序。

### F2 / P0：建立明确且一致的子节点数量契约

#### 现状与影响

| API | 当前行为 |
| --- | --- |
| `Padding::child(a).child(b)` | 后者覆盖前者 |
| `Padding::child(a).with_children(b)` | 返回错误 |
| 宏中的上述错误 | 转成运行时 panic |
| `Align::child(...)` | 可以追加多个 child |
| `Align` 布局 | 只使用第一个 child，并返回一个 placement |
| `SizedBox` 等叶子 | 实现接收空列表、拒绝非空列表的 `WithChildren` |

证据：

- `crates/harbor-widget/src/construction.rs:89–110`
- `crates/harbor-widget/src/widgets/padding.rs:39–58`
- `crates/harbor-widget/src/widgets/align.rs:45–106`
- `crates/harbor-widget-macros/src/expand.rs:82,150`
- `crates/harbor-widget/tests/child_construction.rs`

#### 建议

明确三类组件，并以共享协议表达，而不是让宏维护 widget 名称白名单：

- **Leaf**：不能附加子节点。
- **SingleChild**：一个内容槽。
- **MultiChild**：有序子节点列表。

具体规则：

- 单槽统一用 `.child(value)`，作为属性设置；重复设置统一覆盖。
- 多槽使用 `.children(...)`，追加操作采用明确的方法名，并固定顺序语义。
- 宏和手写 API 使用同一个附加协议，不能一边覆盖、一边报错。
- `Align` 收紧为单子节点；多个内容叠放交给 `Stack`。
- 静态可判断的非法 child 数量尽量编译期报错；动态数量仍需明确检查。
- 删除叶子组件仅用于处理 child list 的样板协议实现。

这是有意的契约变更，必须迁移原先依赖错误或覆盖行为的调用和测试。

### F3 / P0：统一普通 UI 回调的线程模型和事件命名

> 后续实施说明：本节保留原审阅建议。实际契约保留 Button/IconButton 的 `.on_click(...)` 和 InteractiveRegion 的 `.on_activate(...)`，并统一普通控件为 `.disabled(bool)`；见 ADR 0081、0082、spec 0036 及第 9 节 F3 记录。

#### 现状与影响

- `Button::on_click`、`IconButton::on_click`、`MouseRegion::on_enter` 要求 `Send + Sync`。
- `InteractiveRegion::on_activate`、`TextField::on_changed` 没有该要求。
- `Signal` 使用 UI 线程上的 `Rc<RefCell<_>>`。

因此，相同的局部状态更新逻辑，换用 Button 后就不能直接捕获该状态。`docs/widget/learn.md` 需要专门解释这个限制。

证据：

- `crates/harbor-widget/src/widgets/button.rs:27`
- `crates/harbor-widget/src/widgets/icon_button.rs:46`
- `crates/harbor-widget/src/widgets/mouse_region.rs`
- `crates/harbor-widget/src/widgets/interactive_region.rs:119`
- `crates/harbor-widget/src/widgets/text_field.rs`
- `crates/harbor-widget/src/signal.rs`

#### 建议

- 普通 UI 回调统一为 `Fn(...) + 'static`，内部使用 `Rc`。
- `Send + Sync` 只保留在确实跨线程的接口上，不机械修改外部渲染、调度或宿主接口。
- 业务命令仍推荐使用 `Dispatcher`，但它不应成为按钮更新局部 UI 状态的唯一方式。
- 激活统一叫 `.on_activate(...)`，覆盖鼠标、Enter、Space，而非仅 click。
- 状态开关统一为 `.enabled(bool)`，避免同类控件正反命名混用。
- 有焦点能力的控件统一暴露 `.focus_handle(...)`。

需保留事件路由、嵌套控件优先处理、pointer capture 和恰好一次激活的语义。

### F4 / P1：让 Button 支持组合，收敛内容测量与绘制责任

> 后续实施说明：本节保留原审阅基线。Button 单槽组合、共享 ControlSurface 与 TextLabel/Icon 内容测量绘制已落地；旧 `Button::new(text)` 已迁移为 `Button::text(text)`，详见 ADR 0083、spec 0037 及第 9 节 F4 记录。

#### 现状与影响

- Button 是固定文字叶子，缺少 IconButton 已有的部分配置能力。
- InteractiveRegion 不仅处理交互，还测量和绘制 label。
- 其 label 宽度按字符数估算，而 TextLabel 已使用 renderer advances 测量。

不同控件重复承担内容展示责任，既限制组合，也增加行为不一致的机会。

证据：

- `crates/harbor-widget/src/widgets/button.rs`
- `crates/harbor-widget/src/widgets/icon_button.rs`
- `crates/harbor-widget/src/widgets/interactive_region.rs:228–335`
- `crates/harbor-widget/src/widgets/text_label.rs`

#### 建议

分清职责：

- InteractiveRegion：交互状态、激活、焦点、capture。
- 控件表面：背景、边框、内边距和焦点环。
- TextLabel / Icon：内容测量与绘制。
- Button：组合上述能力，支持自定义内容。

提供少量便利构造：

```rust
Button::text("Save")
Button::icon(LucideIcon::Plus, "New")
Button::new().child(custom_content)
```

IconButton 可以作为便利组件保留，但需共享行为和配置，不能独立演化。文字测量收敛可能改变既有尺寸，应使用真实布局与视觉样例验证，不视为纯机械搬迁。

保留现有共享交互状态机的价值，不为 API 统一复制输入处理，也不强制为每种责任增加一个公开类型或额外 Fiber。

### F5 / P1：采用 Flutter 式显式组件树，而非通用链式布局扩展

> 本节是修订后的设计与实施建议，尚未实施。下面区分已核对的 Harbor 现状、Flutter 的设计和建议新增的接口；目标 API 草图不能作为当前可编译示例。具体公开签名须在实施前固定，并与 F6/F7/F8 协调。

#### F5.1：目标——借鉴 Flutter 的组合方式，而非只模仿缩进

希望 UI 声明直接回答三个问题：**有哪些组件、每个组件配置了什么、谁包含谁**。

Flutter 的典型声明是：

```dart
Align(
  alignment: Alignment.topRight,
  child: SizedBox(
    width: 440,
    child: Padding(
      padding: const EdgeInsets.all(8),
      child: content,
    ),
  ),
)
```

这不是给 `content` 逐步附加全局样式，而是创建三个职责明确的组件：Align 定位、SizedBox 施加尺寸约束、Padding 分配内边距。声明从外到内阅读，不需要从链尾反推父节点。

Harbor 应采用相同的组合思路，但保留普通 Rust 表达式与现有 `view!` 子树协议：

```rust
// 目标 API 草图：Insets、Alignment2D 和 SizedBox 单槽能力尚待实现。
view! {
    Align::new(Alignment2D::TOP_RIGHT) => {
        SizedBox::width(440.0) => {
            Padding::new(Insets::all(8.0)) => {
                content;
            }
        }
    }
}
```

因此，F5 的目标不再是“减少嵌套”，而是：

- **布局关系显式化**：接受有语义的嵌套，不用通用方法链隐藏包装。
- **配置声明集中化**：简单组件用具名构造入口；配置较多时用组件专属配置值，减少长属性链。
- **行为保持可解释**：布局约束、绘制顺序、命中范围和身份仍有明确归属，不由宏猜测。

#### F5.2：与 Flutter 的逐项对比

| 维度 | Flutter | Harbor 建议 | 不照搬的部分 |
| --- | --- | --- | --- |
| 布局组合 | `Padding(child: ...)`、`Align(child: ...)` 等具名 widget | `Padding::new(...) => { ... }` 等显式父节点 | 不新增 `content.padding(...).align(...)` 通用布局链 |
| 属性声明 | Dart 构造器具名参数，如 `alignment:`、`padding:` | 简单参数用具名类型；较多参数用普通 Rust 配置结构体字段 | Rust 没有同等的原生具名调用参数，不把 Dart 语法直接塞进宏 |
| 子节点 | 单槽 `child`，多槽 `children` | 宏 block 附加子树；手写 `.child/.children` 共用 F2 能力协议 | 保留 Harbor 的静态数量检查、动态错误策略和替换语义 |
| 固定尺寸 | SizedBox 可有子节点，也可作为空白占位；尺寸服从父约束 | 将现有叶子 SizedBox 扩为可空的 SingleChild，并增加单轴构造入口 | 不把固定宽度解释成可以突破父约束 |
| 布局过程 | “Constraints go down. Sizes go up. Parent sets position.” | 保留已有父驱动测量与 BoxConstraints，不引入另一套 CSS/style 布局模型 | 不为语法改造重写布局引擎 |
| flex 分配 | Expanded/Flexible 提供给祖先 Flex 的 parent data | 显式 Expanded/Flexible；继续遵守 Harbor 的直接布局父级契约 | Flutter 允许路径中存在 StatelessWidget/StatefulWidget；不能据此假设 Harbor metadata 会穿透普通包装 |
| 身份与状态 | widget 的 type/key 决定 element 更新；GlobalKey 另有跨父级迁移能力 | 保留已有 `.keyed(...)` 及 parent-local key/type reconciliation | 不新增 GlobalKey，不承诺跨父级移动自动保状态 |
| 复用 | 提取具名 widget/组合 | 提取 Component 或返回 UI 描述的函数 | 不强制每个 helper 都变成新增状态边界或 runtime 节点 |
| 便利容器 | Container 按固定规则组合布局、装饰和命中行为 | 优先使用已有布局和装饰组件；业务预设留在应用层 | Flutter 确实有 Container；本轮不照搬其整套复合行为，不宣称 Flutter 禁止便利容器 |

重点是学习 Flutter 的**组件组合、局部配置、约束模型**，不是迁入 Widget/Element/RenderObject 的整套运行时，更不是把所有方法名改成 Flutter 的名字。

#### F5.3：已核对的 Harbor 现状与差距

| 位置 | 当前接口/行为 | 需要做的修改 |
| --- | --- | --- |
| `widgets/sized_box.rs` | `SizedBox::new(Size)` 是叶子；只有固定二维 size 和可选 color，没有 child | 增加可空单槽、单轴尺寸表达和父驱动 child 测量；保留无 child 占位用途 |
| `widgets/constrained_box.rs` | `.min_width(w).max_width(w)` 表达固定宽度；支持动态 `max_height_with` | 固定尺寸场景迁到 SizedBox；真正的区间约束和动态高度继续使用 ConstrainedBox |
| `widgets/padding.rs` | `Padding::new(top, right, bottom, left)`、`Padding::all(value)`，已为单槽 | 与 F8 共用 Insets，减少位置参数；保留其已有布局职责，不新增全局 `.padding(...)` |
| `widgets/align.rs` | 一维 Alignment，加 `.vertical_alignment(...)`；另有 `.insets(...)` | 与 F8 协同提供真正的二维定位值；明确现有 inset 行为，不能机械换层 |
| `widgets/row.rs`、`column.rs`、`flex.rs` | 已共用 Flex 引擎，gap/主轴/交叉轴配置靠 builder | 为常见多属性声明提供共享的局部布局配置入口，不复制引擎 |
| `widgets/flexible.rs` | Expanded/Flexible 已为单槽，metadata 不穿透普通布局节点 | 保留组件与现有诊断；主要修改示例和调用结构，不另加 `.expanded()` |
| `construction.rs` | `ComponentExt` 当前已有 `.keyed(...)`，Keyed 转发组件和 child 能力 | 保留现有 key 功能，只禁止继续扩张为通用布局扩展集 |
| `harbor-widget-macros/src/{ast,parse,expand}.rs` | 节点接收普通 Rust 表达式，经共享协议附加 child block | 主方案不需要新的属性语法、widget 名称白名单或运行时分支 |

应用中的实际样板包括：

- `workspace.rs::search_bar`：状态文本固定 120dp；面板固定 440dp 宽及既定高度；顶部对齐通过 vertical override 表达。
- `tab_view_ui.rs::tab_rail`：rail 宽度重复设置 min/max，四方向 padding 使用位置参数。
- `tab_view_ui.rs::close_tab_button`：固定宽度与最大高度混合；不能直接替换为固定二维尺寸。
- `appearance.rs::GlassAppearance::panel`：维护真实滤镜、装饰和裁剪的产品预设；不是单纯的布局样板，不能为“显式树”拆散其业务不变量。

#### F5.4：具体 API 修改方案

##### A. 不新增通用布局扩展，保留必要的现有接口

不在 `ComponentExt` 上新增 `padding/width/height/size/constraints/align/decorated/expanded` 等隐式包装方法。对应能力通过具名组件提供。

**现有 `.keyed(...)` 保留**。它是已有的身份标记入口，不是布局包装方法，不应为了减少链式调用删除，也不需要给每个组件复制一个 key setter。

组件自身的短配置方法可继续存在，例如 `TextField::disabled`、`Button::on_click`。本轮不要求删除所有 builder，不改 F3 已确定的命名与回调契约；新的声明方式应与这些方法写入相同的组件配置，不能形成两套行为。

##### B. 将 SizedBox 从 Leaf 扩为可空 SingleChild

建议保留 `SizedBox::new(Size)`，新增关联构造入口 `SizedBox::width(f32)` 与 `SizedBox::height(f32)`；“只固定一轴”在内部应显式表示为未指定另一轴，而不是以 0 或无穷冒充缺省值。

需要实现的行为：

1. 存储可空 child，提供 `.child(value)`，并实现 F2 的 `WithChildren<Capability = SingleChild>`。
2. 指定轴将期望尺寸夹入父级允许区间，再作为该轴的 tight 约束测量 child；未指定轴继承父约束，并由 child 的测量结果决定尺寸。
3. 无 child 时，未指定轴以零作为自然尺寸，再服从父级最小约束；已有 `new(Size)` 空白占位的行为保持。
4. child 放在局部原点，纯尺寸包装不增加输入处理、焦点停靠点或默认裁剪。
5. 旧 `.color(...)` 绘制能力本轮保留，仍覆盖实际分配的 rect；不要夹带删除既有绘制 API 的重构。
6. 当前公开的 `size` 字段无法完整表达单轴缺省。实施时必须明确字段兼容策略、迁移直接字段访问；建议改为能表达可选轴的内部配置并提供只读访问，不假装这只是无破坏性的构造器增补。

这是对 **SizedBox 个体能力** 的有意变更，不放宽“叶子不能接收 child”的总体规则。`crates/harbor-widget-macros/tests/ui/fail/leaf_blocks.rs` 及 `.stderr` 等将 SizedBox 当作叶子的编译失败 fixture 要改用真正的叶子（如 TextLabel），并新增 SizedBox 单槽通过、超量失败及空 block 清空的测试。

本轮不顺手实现 Flutter 的 `SizedBox.expand/fromSize/shrink` 全家桶，也不新增第二个同义尺寸包装类型。

##### C. Padding/Align 使用具名配置，与 F8 共用类型

- `Padding::new(Insets)` 替代四个裸数；`Insets::all(...)` 和具名字段表达不同方向。已有 `Padding::all(...)` 可保留为便利构造，不是通用扩展链。
- `Align::new(Alignment2D)` 一次表达横纵定位，提供 `TOP_RIGHT/CENTER` 等常用值；不要让只负责定位的二维值包含 Stretch。
- Row/Column 的交叉轴 Stretch 是实际布局能力，不因 Align 的收敛而被删除。
- `TextField::padding` 仍是编辑控件内部配置；外部留白使用 Padding 组件，不能混为同一 API。
- 现有 `Align::insets` 的迁移须保留它缩减可用定位区域的语义。`Padding => Align` 与 `Align => Padding` 在 tight/loose/unbounded 条件下不保证相同，不能只凭最终视觉相似互换。

这些类型的定义和数值验证应与 F7/F8 一次协调，不在 F5 内另做一份 Insets、Alignment2D 或独立校验规则。

##### D. 多属性配置借鉴 Flutter 具名参数，但用普通 Rust 值实现

显式嵌套只解决“树”，不解决 `Row::new().gap(...).cross_axis_alignment(...).main_axis_alignment(...)` 的配置链。建议为 Row/Column/Flex 提供一个仅描述自身布局的共享配置入口。

以下名称是建议签名，**不是已有接口或最终命名承诺**：

```rust
view! {
    Row::with_options(FlexOptions {
        gap: 4.0,
        cross_axis_alignment: Alignment::Center,
        ..Default::default()
    }) => {
        Expanded::new() => {
            TextField::new(controller.clone()).disabled(!enabled);
        }
        Button::text("Close").on_click(on_close);
    }
}
```

对应 Flutter 的阅读方式是：

```dart
Row(
  crossAxisAlignment: CrossAxisAlignment.center,
  children: [
    Expanded(child: TextField(controller: controller, enabled: enabled)),
    TextButton(onPressed: onClose, child: const Text('Close')),
  ],
)
```

这里不要求 gap 在两份示例中采用完全相同的 API；对比的是“本节点属性集中声明、children 单独组织”。

配置入口的具体要求：

- 配置只含 gap、主轴分布、交叉轴对齐等局部布局属性；axis 仍由 Row/Column 或 Flex 构造表达，children 仍走统一 slot 协议。
- 默认值保持当前 Flex 的默认行为；配置值和旧短 builder 共用同一份实际配置与校验路径。
- 不含任意组件通用的 padding、margin、key、事件回调或业务主题；它不是万能 Style。
- 不为所有控件机械生成一套 Props/Options。只对真实多属性调用有收益的组件引入，Button 的一个回调无需因此增加复杂泛型配置类型。
- 不新增宏内 `Row { gap: ..., children: ... }` 属性语言。普通结构体表达式已经能提供字段名、类型检查和 IDE 补全；若未来要求完全仿 Dart 的节点属性语法，应作为单独的宏/ADR 决策，而不是 F5 默认前提。

##### E. 保持装饰、flex 和产品预设的责任边界

- 外部背景、边框和 clip 继续通过 DecoratedBox 等已有组件表达，不通过 `.decorated(...)` 隐式插层。
- Expanded/Flexible 必须处于能消费它的直接布局父级下。通常应写 `Row => Expanded => Padding => content`，不能写成 `Row => Padding => Expanded => content` 并期待 metadata 穿透。
- Flutter 对 Expanded 路径的限制与 Harbor 并非完全相同；本轮不为“兼容 Flutter 写法”新增 parent-data 穿透机制。
- `GlassAppearance::panel` 可以作为有业务意义的组合入口保留。若需调整为返回延迟描述，应另核对其 BuildCx、主题与 generation 生命周期，不借布局迁移顺手改变构建时机。
- 不新增全能 Container 来吸收 padding、尺寸、装饰、滤镜、事件和 key；需要的布局层级必须仍可从声明或具名产品组件契约理解。

#### F5.5：迁移前后应具体长什么样

固定宽度当前常见写法：

```rust
// 现有 API
ConstrainedBox::new().min_width(440.0).max_width(440.0)
```

目标声明：

```rust
// 目标 API
view! {
    SizedBox::width(440.0) => {
        panel_content;
    }
}
```

注意三个不能机械替换的情况：

1. `.max_width(440.0)` 只是上限，不能改成固定 440dp。
2. `.min_width(20.0).max_width(20.0).max_height(24.0)` 是固定宽度加高度上限，不是固定 20×24；保留对应 ConstrainedBox，或组合单轴 SizedBox 与高度上限，并验证新增层级。
3. `max_height_with` 依赖当前字体度量，不能替换成一次求值的静态 height。

推荐先在 `search_bar` 与 `tab_rail` 验证新 API：迁移固定尺寸、Insets、二维定位和 Row/Column 多属性配置；保留 popup generation/source 校验、Dispatcher、controller、focus handle 和 glass preset 的职责。UI 结构改写不能同时改变命令来源或编辑事件顺序。

#### F5.6：宏和运行时哪些不改

- `view!` 仍接受普通 Rust 表达式，`=>` 附加子树，`for/if/match` 组织 children；不解析一份新的属性 DSL。
- 根、child 和内容 slot 继续使用 F1 已落地的描述/延迟构建边界，不新增 Flutter 式 Element 系统。
- 单槽覆盖、空 block 清空、静态数量诊断、动态数量错误继续遵守 F2；宏不硬编码 SizedBox 等 widget 名称。
- 描述表达式仍只求值一次，兄弟顺序、分支惰性及 children-before-parent 构造顺序保持。Flutter 的语法外观不构成改变 Harbor 求值顺序的理由。
- 保留现有 key/type reconciliation 和独立 slot hooks/订阅清理。新增/删除 wrapper 是结构变化，必须逐场景验证，不能承诺任意树形改写天然保状态。

相关约束来自 [ADR 0032](.grimoire/adr/0032-view-macro-rust-expression-grammar.md)、[ADR 0079](.grimoire/adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md) 和 [ADR 0080](.grimoire/adr/0080-unify-child-cardinality-and-replacement-semantics.md)。后续只需修订受影响的个别 widget 能力与公开签名，不据 F5 重新打开这些基础契约。

#### F5.7：实施顺序与修改范围

| 步骤 | 修改位置 | 具体产出 | 验证出口 |
| --- | --- | --- | --- |
| 1. 固定目标签名与基线 | 本节、相关 F2/F7/F8 契约、child/layout/macro 测试 | 定下 SizedBox 单槽及单轴缺省、Insets/Alignment2D、FlexOptions 候选接口；记录原几何/路由/key 行为 | 每个 breaking change 有迁移表；代表性场景有基线 |
| 2. 尺寸组件 | `widgets/sized_box.rs`、必要的共享约束辅助与 construction 测试 | 可空 SingleChild、width/height 构造、child 测量、原占位/color 行为与字段访问迁移 | 父约束、单轴/双轴/无 child、数量和替换测试通过 |
| 3. 具名配置 | `layout/`、`widgets/{padding,align,flex,row,column}.rs`、相关配置测试 | 共用 Insets/Alignment2D，局部 Flex 配置入口；复用现有引擎与验证规则 | 新旧等价输入具有相同布局，必要差异有明确契约 |
| 4. 应用迁移 | `harbor-app/src/{workspace,tab_view_ui,appearance}.rs` 及真实受影响调用 | 先迁 search/rail，检查混合约束；保留有意义的 helper 和产品预设 | 窄窗口、文本变化、popup、嵌套关闭与焦点回归通过 |
| 5. 导出和文档收尾 | `lib.rs`、`widgets/mod.rs`、F6 prelude、README、learn/catalog、宏 fixtures | 一套推荐声明风格；修正 SizedBox 原 leaf fixture；清晰区分支持的短配置与不新增的通用扩展 | 新示例可编译，旧入口按确认的策略迁移，跨 crate/依赖重命名不回归 |

步骤 2/3 的调用迁移可随 breaking change 原子提交，不要求仓库经过无法编译的中间态。修改宏生产代码不是预设任务：若普通表达式和现有 capability 已足够，只调整覆盖和示例。

#### F5.8：验收与边界场景

- **尺寸服从父级**：期望宽 440dp，父级最大 300dp 时结果为 300dp；父级 tight 500dp 时不能强行缩成 440dp；父级松约束允许时才取 440dp。
- **单轴不是二维固定**：仅设 width 时高度跟随 child；无 child 时未设轴以零为自然值并服从父级最小值。内容改变和约束收紧/放宽后结果正确。
- **数值错误不被藏起**：NaN、无穷、负值及溢出遵循 F7 最终确定的边界校验策略；不以新便利入口偷偷忽略、归零或改成无条件 panic。
- **结构与 slot**：SizedBox 的普通根、宏单 child、手写 child、替换和空 block 清空均正确；静态两个 child 编译失败；动态超量沿用 F2 的错误路径。
- **语义顺序**：交换 Padding/DecoratedBox/clip 等节点能够产生各自约定的区别；实现不得优化成与声明无关的统一包装。比较场景、clip 和命中范围，不只比较外框尺寸。
- **flex**：Expanded 的正确层级分配到剩余空间；被普通 wrapper 隔开的错误层级继续得到既有诊断；无界主轴与 overflow 行为保持现有规则。
- **身份与生命周期**：keyed 列表插入/重排保持应保持的状态和焦点；不新增 Tab stop；检查主题继承、外部 draw/IME 注册及 slot 替换/卸载清理。树形改写可能导致的 remount 必须被识别，而不是由 key“兜底”。
- **应用回归**：搜索框输入、状态文本固定槽、Enter/Shift+Enter/Escape、关闭按钮与 popup generation 校验不变；rail 宽度与滚动、嵌套关闭不触发父行激活；glass 的滤镜/clip/命中顺序不变。
- **编译与集成**：宏/child/layout 测试、跨 crate fixtures、文档示例及受影响 workspace 构建通过；需要真实渲染的布局/clip 差异补 GPU 证据，涉及焦点/IME/HMR 的集成项单列人工验收，不能由 headless 通过替代。

完成标准是“树结构可读、配置集中、约束与生命周期可验证”，不是 LOC 或嵌套层数下降。

实施后建议执行以下检查（**不是本轮已运行的结果**），并记录单独的 native/GPU/HMR 验收证据：

```sh
cargo test -p harbor-widget -p harbor-widget-macros -p harbor-app --all-features
cargo check --workspace --all-targets --all-features
python scripts/docs/check_examples.py --include-readmes
cargo fmt --all -- --check
git diff --check
```

#### F5.9：Flutter 参考与方案边界

本节对照以下官方文档中的设计；这些是设计参考，不是 Harbor 已有能力的证明：

- [Understanding constraints](https://docs.flutter.dev/ui/layout/constraints)：父约束、child 尺寸与父级定位。
- [Padding 构造器](https://api.flutter.dev/flutter/widgets/Padding/Padding.html)：具名 padding、child 与 key。
- [SizedBox](https://api.flutter.dev/flutter/widgets/SizedBox-class.html)：可选 child、可选轴与父约束优先。
- [Expanded](https://api.flutter.dev/flutter/widgets/Expanded-class.html)：Flex parent data 及路径限制。
- [Widget.key](https://api.flutter.dev/flutter/widgets/Widget/key.html)：type/key 更新规则及 GlobalKey 的额外能力。
- [Container](https://api.flutter.dev/flutter/widgets/Container-class.html)：便利组合存在，但其布局、绘制与命中具有具体规则，不能当成任意合并 wrapper 的依据。

本轮只修订方案，不实现上述 API，不修改 F4；新类型和签名仍需实施前确认。若后续要求完全去掉控件自身的 builder，或引入 Flutter 式宏属性语法，应单独明确范围与 ADR 取舍，不能视为本节已授权。

### F6 / P1：统一构造器、导入入口和配置命名

#### 现状与影响

当前同时存在：

```rust
Focus::empty()
Focus::new(child)
Shortcuts::empty()
Shortcuts::new(child)
Actions::handler(handler)
Actions::new(child, handler)
```

部分常用类型从 crate 根导出，Padding、Align、Stack、TextLabel 等则需要深入模块导入。

证据：

- `crates/harbor-widget/src/widgets/{focus,shortcuts,actions}.rs`
- `crates/harbor-widget/src/lib.rs`
- `crates/harbor-widget/src/widgets/mod.rs`
- `docs/widget/catalog.md`

#### 建议

- 构造器只接收必要配置，不接收 child。
- child 经统一 slot API 添加。
- 可选行为使用 builder 方法。
- 增加受控的 `harbor_widget::prelude::*`，覆盖日常组件、Component、View 和扩展 trait。
- 根导出与模块导出形成明确规则，不靠调用者猜测。

不建议为了名称更短而把全部类型重新命名；重命名应解决具体歧义。

### F7 / P1：收敛数值校验与错误暴露策略

#### 现状与影响

非法数值的处理可能是：

| 入口 | 行为 |
| --- | --- |
| `Icon::size` | 忽略非法值 |
| `TextField::padding` | 非法值归零 |
| `BackdropFilter::new` | 构造时返回错误 |
| `Flex::gap`、`Separator::thickness` | 布局时报错 |

调用者难以从 API 外观判断错误何时暴露。

#### 建议

- 配置输入在边界通过 `try_*` 或校验类型处理。
- widget 尽可能接收已验证配置。
- 依赖父约束才能判断的问题继续在布局阶段诊断。
- 不再随意静默忽略或归零。
- 不为链式调用美观而把所有错误改成 panic。
- 共享校验规则，但不为每一个数字创建独立类型。

这会改变非法输入的可观察行为，需明确迁移规则，并补充 NaN、无穷、负值、溢出等测试。

### F8 / P2：用具名类型替换容易误用的位置参数

建议：

| 当前接口 | 建议 |
| --- | --- |
| 四个 `f32` 的 padding/insets | `Insets` |
| Align 的一维 Alignment 加 vertical override | `Alignment2D` |
| `on_vertical_navigation(bool)` | `NavigationDirection` |
| material 的 `[f32; 4]`、`[f32; 2]` | 具名 tone/rim 配置 |

证据：

- `crates/harbor-widget/src/widgets/padding.rs:18`
- `crates/harbor-widget/src/widgets/align.rs`
- `crates/harbor-widget/src/layout/align.rs`
- `crates/harbor-widget/src/widgets/editable_text.rs`
- `crates/harbor-widget/src/widgets/backdrop_filter.rs:58–106`

`Alignment::Stretch` 不应出现在只定位、不实际拉伸的 API 中。类型应阻止具体误用，而不是增加没有约束价值的包装。

## 4. 宏的目标方向

保留现在的主体：

- 普通 Rust 表达式。
- `=>` 表达子树。
- `for`、`if`、`match` 组织子节点。
- 显式所有权，无隐式 clone。
- 解析/展开与运行时分离。

不建议同时引入 JSX、独立属性语言或宏内部状态系统，否则会把 API 的问题转成另一套 DSL 的复杂性。

优先改造：

1. 去掉根节点立即构建的特殊规则。
2. 去掉必须传递 `cx` 的要求。
3. 统一根与子节点接受的描述类型。
4. 尽早暴露静态错误，诊断指向具体节点。
5. 保留表达式求值一次、兄弟顺序、分支惰性执行等保证。
6. 对目前测试固定的 children-before-parent 构造顺序做显式处置，不借重构无意改变副作用顺序。

以下是目标 API 草图，**不是当前可编译代码**；SizedBox 单槽、Insets、Alignment2D、FlexOptions 与 `with_options` 均按 F5 的候选方案展示，不代表已存在的接口：

```rust
use harbor_widget::prelude::*;

view! {
    SizedBox::width(440.0) => {
        Padding::new(Insets::all(8.0)) => {
            Column::with_options(FlexOptions {
                gap: 8.0,
                ..Default::default()
            }) => {
                TextLabel::new("Search");

                Row::with_options(FlexOptions {
                    gap: 4.0,
                    cross_axis_alignment: Alignment::Center,
                    ..Default::default()
                }) => {
                    Expanded::new() => {
                        TextField::new(controller.clone()).disabled(!enabled);
                    }

                    Button::icon(LucideIcon::X, "Close")
                        .on_click(move |_| actions.dispatch(Action::Close));
                }
            }
        }
    }
}
```

目标不是少几个符号，而是让调用者不再手动安排 `.build(cx)` 和类型转换，并能从显式组件树直接读出布局关系；组件自身的属性配置不等于通用链式布局包装。

## 5. 依赖有序的实施单元

下列是建议计划，不是本次已执行工作。开始实施前需要确定最终公开签名和构建模型。

### U1：固定基础契约

- 对应：F1、F2、F3、F6、F7、F8 的共享约定。
- 写入所有权：`construction.rs`、公开 API 契约/测试和相关设计文档。
- 读取依赖：宏、View、reconciler、widgets、现有消费者。
- 产出：slot 分类、key 身份、构建边界、回调约束、命名与校验策略。
- 保留：有序子节点、显式所有权、业务与 widget 边界。
- 验证：先固定状态、主题、child 数量、求值顺序、卸载的 characterization 场景。
- 预期收益：后续迁移不再分别定义共享语义。

### U2：统一控件与子节点 API

- 对应：F2、F3、F4、F6。
- 前置：U1。
- 写入所有权：基础 widgets、construction 协议实现、相关消费者和测试。
- 读取依赖：event routing、focus/capture、主题与文本测量接口。
- 保留：嵌套控件优先处理、恰好一次激活、disabled/focus、滚动边界传递。
- 验证：局部 Signal 可被 UI 回调捕获；鼠标/键盘激活一致；关闭子按钮不激活父行；macro/builder child 语义一致。
- 预期收益：同类控件可以互换，减少样板协议与重复内容展示路径。

### U3：显式布局组合与具名配置

- 对应：F5、F7、F8。
- 前置：U1；依赖 U2 已稳定的 child 接口。
- 写入所有权：SizedBox 单槽/尺寸测量、具名布局组件、局部 Flex 配置类型及相关声明式调用；不新增通用 `ComponentExt` 布局包装方法集，保留现有 `.keyed(...)`。
- 读取依赖：父约束、flex parent data、clip/paint/hit-test 契约。
- 保留：显式包装顺序、父约束权威、key/state、命中区域与焦点数量。
- 验证：按 F5.7 的依赖顺序落实 F5.8 验收；显式树与等价手写 slot 的几何/场景/事件一致；覆盖单轴尺寸、父约束优先、SizedBox 的 Leaf → SingleChild fixture 迁移及多属性配置等价；非法输入策略与 F7 一致。
- 预期收益：组件树直接呈现布局关系，减少配置样板和参数误用，而非压平有语义的嵌套。

### U4：统一构建链与宏

- 对应：F1 和宏改造。
- 前置：U1、U2；宏使用的共享协议须先稳定。
- 写入所有权：`view.rs`、`fiber/reconcile.rs`、相关 runtime 构建路径、宏的 AST/parse/expand 和契约测试。
- 读取依赖：外部注册、hook/subscription 生命周期、主题、根替换、HMR。
- 集成方式：作为关联单元串行处理共享文件，不能仅合并宏改动。
- 保留：身份与状态、订阅清理、主题继承、外部回调注册与卸载顺序。
- 验证：根/子节点同义；状态独立；keyed 重排保状态和焦点；返回树支持所选描述形式；求值顺序和宏卫生。
- 预期收益：UI 提取和组合不再要求调用者理解 eager/deferred 的位置差异。

### U5：应用迁移、文档与旧路径移除

- 对应：全部发现的消费者迁移和收尾。
- 前置：相关基础接口稳定；最终验收在 U2–U4 集成树上执行。
- 写入所有权：tabs、search、palette、confirmation、appearance、示例与开发者文档。
- 读取依赖：业务命令边界、终端桥接、native host/HMR 契约。
- 保留：业务状态与效果顺序、终端/session 身份、焦点、IME、滚动和确认权限。
- 验证：实际调用可读性、受影响构建、交互测试、原生窗口与支持的 HMR 场景。
- 预期收益：仓库只维护一套新契约，不长期保留双 DSL、旧入口或无价值兼容层。

## 6. 验证要求

实施时至少需要覆盖：

- 宏编译通过/失败诊断，包括跨 crate 和依赖重命名。
- 解析与展开卫生、求值次数、顺序、分支可达性、所有权移动。
- macro 与手写 builder 的结构、布局、场景与事件等价。
- keyed 插入、删除、重排后的状态和焦点保留。
- 内容 slot 的状态独立性与主题继承。
- 单子节点、叶子、多子节点规则及动态数量错误。
- Button/IconButton/自定义内容的激活、disabled、焦点环、capture、嵌套关闭行为。
- 显式布局组件的 padding/clip/paint/hit-test 层级与 flex metadata；不依赖通用链式包装扩展。
- 文本测量变化对长标题、多语言文字和窄空间的影响。
- 外部 draw/schedule/IME 注册与 unmount 清理。
- 原生输入、剪贴板、IME、终端嵌入与 HMR teardown。

可从现有 `macro_tests.rs`、`tests/child_construction.rs`、宏 crate 的 trybuild 用例及交互/焦点/主题测试扩展。未运行或无法运行的检查必须单列，不能算通过。

## 7. 不建议做的事情

- 只把分号和箭头换成 JSX，而保留原有构建差异。
- 引入万能 Context、万能 Style 或新的全局事件总线。
- 为减少文件数删除共享交互状态机或 widget/application 边界。
- 把所有包装合并成一个组件，隐藏顺序、命中、clip 和 flex 语义。
- 把所有回调都改成跨线程，或把所有错误都改成 panic。
- 将 Harbor 产品玻璃效果、命令意义或终端模型移进通用 widget。
- 为追求 LOC 降幅删除必要测试、生命周期处理或有价值的便利组件。

## 8. 优先级与审阅状态

如果先做三件事：

1. 统一构建边界。
2. 统一 child 契约。
3. 去掉普通 UI 回调的多余线程约束。

建议保留普通 Rust 表达式语法、共享交互状态机、controller/editing-core/field 分层、共享 flex 引擎，以及应用 → widget 的依赖方向；这些边界具有实际价值，不应仅因实现较复杂而删除。

本次审阅已覆盖所选生产 API 范围，未进行全仓库正确性审计。**状态更新（2026-10-23）：F1 已实施并完成所记录的自动化验证（spec 0034 / ADR 0079）；F2 已实施并通过自动化验证（spec 0035 / ADR 0080）；F3 已实施并通过自动化验证及 native hidden-host smoke，仍有原生 IME/HMR 验收缺口（spec 0036 / ADR 0081、0082）；F4 已实施并通过自动化及 GPU/系统字体渲染验证，仍有原生应用焦点/IME 与实际 HMR 验收缺口（spec 0037 / ADR 0083）；F5–F8 仍为待实施建议。** 审阅当时的结论见上文原文。

原建议中 U1 的构建边界与 child 契约、U2 的 child API、U4 的相关宏改造及对应消费者迁移已随 F1/F2 落地；F3 进一步落实了 U1/U2 的普通 UI 回调、控件命名和焦点转发契约，以及对应 U5 调用与文档迁移；F4 落实了 U2 的 Button 内容组合、共享表面与内容测量责任收敛，以及对应 U5 消费者和文档迁移。这不代表 U1–U5 整体完成。后续仍需单独推进 F5–F8，并保留 F1/F3/F4 记录中的原生与 HMR 集成验证待办。每个阶段应独立验证，避免同时改变构建、布局和事件行为而无法定位回归。

## 9. 实施状态记录(2026-10-23)

### F1:已实施并验证

- **产物**:`.grimoire/spec/0034-deferred-view-root-and-content-slot-boundary.md`(In Progress)、`.grimoire/adr/0079-defer-root-construction-and-unify-content-slot-boundaries.md`(Testing,部分取代 ADR 0032 的根构建条款)、`.grimoire/plans/0045-deferred-view-root-and-content-slot.md`。
- **已落地**(一次性迁移,共 54 个文件):
  - 宏:`view! { root }`,不再传 `cx`;根与子节点统一经 `IntoChildView` 转换,已构建 `View` 可作为根;旧签名以聚焦诊断拒绝(trybuild 用例覆盖)。
  - 运行时:`Runtime::set_root` 放宽为接受 `IntoChildView`(组件或 `view!` 描述,向后兼容);根 Fiber 本就由 Runtime 的 `set_root`/`rebuild_root` 持有,未重设计;新增 `materialize_for_fiber`(见下面的契约修订)。
  - 内容 slot:`InteractiveRegion::content` 经内部 `ContentSlot` 组件在独立子 Fiber 构建——hooks/订阅归 slot,主题继承,稳定 key `harbor-content-slot`;移除 tab row 的零 padding 隔离包装并验证等价。
  - 文档:`docs/widget/learn.md`、`concepts.md`、`crates/harbor-widget/README.md` 已同步。
- **验证**:`cargo test --workspace` 56/56 全绿(运行时等价、keyed 重排、事件路由、trybuild、slot 隔离/卸载、求值顺序);`cargo fmt`;workspace 零警告。原生窗口、IME、HMR 实机会话未在本环境运行,人工集成验证通过后 ADR 0079 → Completed、spec 0034 → Completed。

### F1：相对本文建议的契约修订

- F1"风险与前置条件"指出的"组件返回描述与 Fiber 身份关系"已在实施中定为:**build 返回的 deferred 描述在同一 Fiber 上有界原地物化**(深度上限 8,超限报错而非死循环),取代"deferred build 返回 panic"的初稿措辞。这保持"无第二描述类型"和"无边界循环展开仍禁止"的约定;修订已写回 ADR 0079 D3、spec 0034 R7 与 CONTEXT 的 Deferred Root 条目。
- 宏根 Fiber 的 TypeId 现在等于挂载描述组件的类型,而非展开后的具体根组件类型;布局/场景/事件输出已由等价测试确认不变。

### F2：已实施并通过自动化验证

- **产物**：`.grimoire/spec/0035-unified-child-cardinality-and-attachment.md`、`.grimoire/adr/0080-unify-child-cardinality-and-replacement-semantics.md`、`.grimoire/plans/0046-unified-child-cardinality-and-attachment.md`（含 R1–R7 验收证据）。本次仅同步本文状态，未修改关联 spec/ADR 的状态字段。
- **已落地**：
  - 公共协议：`WithChildren::Capability` 由组件声明 `SingleChild` / `MultiChild`，`Keyed` 转发能力与 key；叶子不再实现仅用于接收空列表或拒绝子节点的附加协议，无 widget 名称白名单。
  - 设置与追加：单槽 `.child(value)` 覆盖；多槽 `.children(values)` 替换列表，`.push_child(value)` / `.extend_children(values)` 显式追加。宏 block 是一次替换，空 block 清空；普通值表达式保留已有内容。旧多槽 `.child(...)` 调用及相关测试、文档已迁移。
  - 数量诊断：静态数量经公共 const-count 约束检查；`if` / `match` 分支按互斥备选计算，不相加。单槽静态超量和所有叶子 block 在编译期拒绝，诊断定位接收表达式。动态列表只验证本次传入数量，手写路径返回 `ChildConstructionError`，普通 `view!` 在构造处明确 panic，不截断、不留到布局阶段。
  - 真实槽位：`Align` 改为 `Option<View>` 单槽；`InteractiveRegion` 的 child/content 相互替换，空 block 可清空任一选择。原 ContentSlot 独立 Fiber 的 hooks、主题、订阅及卸载边界保持不变。
  - 不变量：保留单次求值、兄弟顺序、children-before-parent、分支惰性、显式所有权及 deferred 构建；覆盖已暂存但被替换表达式的求值，以及 macro/builder 的结构、key、几何、场景与路由等价。
- **最终验证**：
  - `cargo test -p harbor-widget -p harbor-widget-macros -p harbor-app`：1,193 项通过，0 失败；含 19 个 trybuild 编译通过/失败 fixtures 与依赖重命名检查。最初跳过 3 项 opt-in 测试。
  - `cargo test -p harbor-app --lib -- --ignored`：补跑的 2 项 GPU 像素测试均通过，以上两次命令共 1,195 项通过。
  - `cargo check --workspace --all-targets`、`cargo fmt --check`、所有改动 Rust 文件（含 fixtures）的 rustfmt 检查及 `git diff --check` 均通过；仍有既有 unused-import/variable 警告。
  - 已完成实施、测试、风险审阅、需求核对及格式化后的集成复验，无已确认遗留阻塞。
- **未运行与范围限制**：需要交互桌面的 native hidden-host 生命周期 smoke test 未运行；本轮未进行原生窗口、IME 或 HMR 人工会话验证，也未实现 F3–F8。该结果不替代 F1 已记录的人工集成验证待办。

### F3：已实施并通过自动化验证，原生验收仍有缺口

- **产物**：`.grimoire/spec/0036-ui-thread-callbacks-and-control-api-consistency.md`、`.grimoire/adr/0081-preserve-click-naming-and-standardize-disabled-builders.md`、`.grimoire/adr/0082-ui-thread-local-widget-callbacks.md`、`.grimoire/plans/0047-ui-thread-callbacks-and-control-api.md`（含 R1–R5 对照及验证证据）。关联 spec/ADR 状态字段未修改，不据局部验证宣称整体验收完成。
- **相对原建议的修订**：保留 Button/IconButton `.on_click(...)`，涵盖既有鼠标与 Enter/Space 逻辑激活；InteractiveRegion 继续使用 `.on_activate(...)`。可用性统一沿用 `.disabled(bool)`，而非原建议的 `.enabled(bool)`；无关的 Focus `.enabled(...)` 保持不变。
- **已落地**：
  - 普通 UI 回调：Button/IconButton 点击、MouseRegion enter/exit、IconButton `color_with` 和 Notification dismissal 采用 `Rc` 存储与 `Fn(...) + 'static`，可直接捕获 Signal 或其他 UI-local 状态；不改变真实跨线程接口、外部渲染和调度的线程契约。
  - 编辑器：TextField/EditableText 改为 `.disabled(bool)`，默认仍可用；旧 `.enabled(...)` 方法移除且不保留别名。调用按等价极性迁移，read-only 的选择/复制与 disabled 的编辑/焦点限制保持独立。
  - 焦点：Button/IconButton `.focus_handle(FocusHandle)` 直接转发到既有 InteractiveRegion，不添加 Focus 包装、额外 Fiber 或 Tab stop；共享交互状态机及路由、capture、键盘时机保持不变。
  - 上层调用：Notification composition 保留已配置组件，删除额外 Arc 与纯转发回调；逐一检查 tab/search/palette/confirmation，保留具有视觉、generation/source、生命周期或 Host 协调职责的 helper 与同步状态。业务命令仍通过 Dispatcher，由 Host 按原有顺序执行。
  - 文档与覆盖：Learn 增加直接捕获局部 Signal 的按钮示例，保留业务 intent 与 Host drain/wakeup 说明；catalog 和编辑器文档同步。新增公共 API 编译通过/失败 fixtures、局部状态真实控件交互、焦点遍历/恢复、回调替换/移除/根卸载释放及确认权限回归测试。
- **最终验证**：
  - `cargo test -p harbor-widget -p harbor-app -p harbor-widget-macros --all-features`：1,203 项通过，0 失败，3 项忽略；包括公共 API fixtures、HMR lifecycle 单元测试和应用交互回归，不代表真实 DLL 卸载验收。
  - `cargo test -p harbor-widget --features winit --test native_host_smoke -- --ignored --nocapture`：另行补跑上述忽略项中的 hidden-host smoke，1 项通过，覆盖真实隐藏窗口 GPU 呈现及重复次级窗口生命周期。
  - `cargo check --workspace --all-targets --all-features` 通过；文档示例检查（含 crate READMEs）18 项通过；13 个改动/新增 Rust 文件的 rustfmt 检查及 `git diff --check` 通过。既有 unused-variable 警告未作为本次清理范围。
  - 实施、测试、inline 风险审阅与需求核对、格式化后的集成复验均已执行，无已确认代码阻塞。日志位于 `target/spec0036-final-tests.log`、`target/spec0036-final-check.log`、`target/spec0036-native-smoke.log` 和 `target/spec0036-doc-examples.log`。
- **未运行与范围限制**：未运行 live native IME/candidate-window 交互、实际 HMR 旧 generation 回调销毁先于 DLL 卸载的验收，以及本轮忽略的 2 项应用 GPU 像素测试。headless IME/root teardown 与 native host smoke 不替代这些证据；F3 尚不能标记为整体验收完成。F4–F8 未纳入本轮实施。

### F4：已实施并通过自动化与渲染验证，原生验收仍有缺口

- **产物**：`.grimoire/spec/0037-composable-buttons-and-content-measurement.md`、`.grimoire/adr/0083-composable-buttons-and-shared-control-surfaces.md`、`.grimoire/plans/0048-composable-buttons-and-content-measurement.md`（含 R1–R6 对照、实施修正与验证证据）。spec/ADR 的 Draft / Proposed 状态及早期 spec-only 说明未在本轮改写；本文记录后续实施，不据自动化结果宣称整体验收完成。
- **已落地**：
  - Button 单槽组合：支持 `new()`、`text(...)`、`icon(icon, text)`、`.child(...)`、状态驱动 `.content(...)` 与 `.style(...)`；共享 `SingleChild` 协议，child/content 相互替换，宏空 block 清空，静态/动态数量错误沿用 F2 规则。旧 `Button::new(text)` 移除，无兼容别名，消费者与文档已迁移。
  - 职责收敛：私有 `ControlSurface` 通过 InteractiveRegion 为 Button/IconButton 共享 padding、最小尺寸、背景、边框及子内容之后的焦点环；保留 region 的 `.style(...)` 外观入口与唯一交互状态机，不强制新增公开 wrapper 或焦点停靠点。region 不再存储、估算或绘制 label，内容交给 TextLabel/Icon。
  - 测量与窄空间：内置文字单行、使用 TextLabel renderer advances 与 ellipsis，放宽约束后恢复源文本；图文组合先分配 icon 与间距，再约束文字。为避免有界 Flex 填满最大宽度，采用私有纯布局 IconText arrangement，保留短按钮的内容驱动宽度；极窄父约束仍具权威。
  - 主题与生命周期：Button/IconButton 保留各自 theme role、style/tint 配置与 IconButton 不绘制的 label metadata；自定义内容颜色不被覆盖。状态内容保留独立 Fiber/hooks、主题继承及替换/清空/卸载清理，既有 `.on_click(...)`、`.disabled(...)`、focus handle、capture、嵌套优先处理与键盘激活逻辑继续由共享 region 承担。
  - 应用与文档：confirmation 等 Button 调用及 README/learn/catalog 同步；覆盖文字、图文、自定义 child 与状态内容示例。tabs/search/palette 继续使用既有 region/material 组合，不重复应用 padding，不转移 Dispatcher/Host 业务职责。
- **最终验证（实施记录）**：
  - `cargo test --workspace --all-features`、`cargo check --workspace --all-targets --all-features` 通过；既有 ignored/manual 项未据此计为通过，仍有既有 unused-import/variable 警告。日志：`target/f4-final-tests.log`、`target/f4-final-check.log`。
  - `cargo test --test composable_button_rendering -- --ignored --nocapture` 显式通过，使用真实 GPU/系统字体；`cargo test -p harbor-app --lib -- --ignored --nocapture` 补跑的 2 项 search/palette GPU 验收测试均通过。日志：`target/f4-rendering-final.log`、`target/f4-app-rendered-final.log`。
  - 渲染证据：Radeon RX 7900 XT/Vulkan 下，四个 i/W 的实测文字宽分别为 11.62/44.84dp，对应文字按钮宽 43.62/76.84dp，图文按钮宽 71.62/104.84dp。`target/f4-buttons/samples.png` 已检查，覆盖 320/150/40dp 宽度、四类内容、六种交互状态及长文字/中文/混合 grapheme；几何变化来自实际 advances，不再追求旧字符数估算的像素等价。
  - mounted/编译测试覆盖 slot 替换/清空、macro/builder 等价、数量诊断、旧构造器拒绝、主题/自定义颜色、窄到宽恢复、keyed 状态/焦点、订阅与捕获释放、全表面命中、嵌套拒绝及鼠标/键盘时机。结构审阅确认没有保留 region label renderer 或复制输入处理。
  - `python scripts/docs/check_examples.py --include-readmes`：19 项通过；`cargo fmt --all -- --check` 与 `git diff --check` 通过。全仓 `check_docs.py` 因本报告原有中文字符策略冲突失败；实施阶段改动的 3 份文档 scoped 检查通过，不将全仓文档检查算作通过。
- **未运行与范围限制**：未执行原生应用焦点/IME 交互会话，也未收集实际受支持 HMR generation 回调销毁先于 DLL 卸载的生命周期证据；本轮未单独补跑 native hidden-host smoke。GPU 渲染、headless 生命周期及全特性测试不替代这些验收，F4 尚不能标记为整体验收完成，也不关闭 F1/F3 的既有缺口。F5–F8 未纳入本轮实施；本次报告同步未重新运行上述 Rust 验证。

### 其余发现

- F5–F8 仍为待实施建议；U1 的构建边界、child 契约与普通 UI 回调/命名契约已分别由 ADR 0079、0080、0081/0082 固定，F4 的 Button 内容组合与共享表面契约由 ADR 0083 / spec 0037 固定。布局便利 API、通用构造器/导出、数值校验与具名配置尚需单独推进；F4 的定向 Button 构造器迁移不代表 F6 整体完成。沿用本文件的证据与验证要求，不将 F4 的实施范围扩大到这些发现。
